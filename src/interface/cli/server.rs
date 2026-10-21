use std::time::Duration;

use tracing::{error, info};

use crate::{
    application::{
        delete_media::DeleteMediaService,
        import_local_store::ImportLocalStore,
        media_source_observation::ProcessObservationService,
        recorded_import::RecordedImportService,
        sync_strm::{SyncStrmConfig, SyncStrmService},
    },
    error::{AppError, AppResult},
    infrastructure::{
        cache::Cache, event::publisher::EventBusPublisher, event_bus::EventBus,
        fs::tokio_file_store::TokioFileStore,
    },
    interface::{
        http,
        http::console::{self, ConsoleContext},
        http::media::{self, MediaServerContext},
        library_sync::LibrarySyncController,
        runtime::MediaDownloadUrlService,
        telegram::{
            self,
            delivery::TelegramDeliveryContext,
            handler::{ProcessMediaSourcesHandler, on_process_media_sources},
        },
    },
    util::signal::shutdown_signal,
};

use super::{context::CliContext, logger};

pub(super) async fn run(data_dir: &str) -> AppResult<()> {
    run_with_shutdown(data_dir, shutdown_signal("server")).await
}

pub(super) async fn run_with_shutdown(
    data_dir: &str,
    shutdown_trigger: impl std::future::Future<Output = ()> + Send + 'static,
) -> AppResult<()> {
    let ctx = CliContext::new(data_dir)?;
    let config = ctx.config();

    logger::init(config.get_log_dir().as_str());

    let db = ctx.db().await?.clone();

    // Infrastructure
    let event_bus = EventBus::new(db.clone());
    let mut bot = teloxide::Bot::new(config.get_telegram_config().bot_token.as_str());
    if let Some(api_url) = config.get_telegram_config().get_api_url() {
        let url = reqwest::Url::parse(api_url).map_err(|e| {
            AppError::InvalidParameter(format!("invalid telegram api_url '{api_url}': {e}"))
        })?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(AppError::InvalidParameter(format!(
                "invalid telegram api_url '{api_url}': scheme must be http or https"
            )));
        }
        if url.cannot_be_a_base() {
            return Err(AppError::InvalidParameter(format!(
                "invalid telegram api_url '{api_url}': url cannot be a base"
            )));
        }
        bot = bot.set_api_url(url);
    }
    let cache = Cache::new(db.clone());
    let library_gateway = ctx.library_gateway();

    // Telegram
    let user_id = config
        .get_telegram_config()
        .user_id
        .try_into()
        .map(teloxide::types::UserId)
        .map_err(|_| AppError::InvalidParameter("invalid telegram user id".to_owned()))?;

    // Emby proxy
    let emby_proxy_config = if config.get_emby_proxy_config().is_enabled() {
        let upstream_base_url = config
            .get_emby_proxy_config()
            .get_upstream_base_url()
            .ok_or_else(|| {
                AppError::InvalidParameter(
                    "emby_proxy.upstream_base_url is required when emby_proxy.enable is true"
                        .to_string(),
                )
            })?;
        Some(EmbyProxyConfig {
            addr: config.get_emby_proxy_config().get_addr(),
            upstream_base_url,
            api_key: config
                .get_emby_proxy_config()
                .get_api_key()
                .map(str::to_owned),
            advertise_base_url: config.get_media_server_config().get_advertise_base_url(),
            strm_path_prefix: config
                .get_media_server_config()
                .get_strm_path_prefix()
                .to_string(),
        })
    } else {
        None
    };

    // Library
    let library = SyncStrmConfig {
        remote_path: config.get_library_config().remote_path.clone(),
        local_path: config.get_library_config().local_path.clone(),
        strm_download_url: config.get_media_server_config().get_strm_download_url(),
    };
    let media_server_addr = config.get_media_server_config().get_addr();
    let media_server_strm_path_prefix = config
        .get_media_server_config()
        .get_strm_path_prefix()
        .to_string();

    // Media server
    let media_server = media::new_router(MediaServerContext::new(
        media_server_strm_path_prefix,
        MediaDownloadUrlService::new(cache.clone(), library_gateway.clone()),
    ));

    let library_update_notifier = ctx.library_update_notifier();
    let delete_media_service = DeleteMediaService::new(
        library_gateway.clone(),
        ImportLocalStore::new(
            TokioFileStore,
            library.remote_path.clone(),
            library.local_path.clone(),
            library.strm_download_url.clone(),
        ),
        library.remote_path.clone(),
        library_update_notifier.clone(),
    );

    // Telegram bot runtime
    let bot_runtime = telegram::BotRuntime::new(telegram::BotRuntimeArgs {
        user_id,
        notify_service: EventBusPublisher::new(event_bus.clone()),
        event_bus: event_bus.clone(),
    });

    // Event delivery
    let event_delivery_telegram = TelegramDeliveryContext {
        bot: bot.clone(),
        user_id: config.get_telegram_config().user_id,
    };
    let subscription_repo = ctx.subscription_repo().await?;
    let notify_service = EventBusPublisher::new(event_bus.clone());
    let event_delivery_media_handler = ProcessMediaSourcesHandler {
        processor: ProcessObservationService::new(
            ctx.file_index_service().await?,
            RecordedImportService::new(ctx.import_record_repository().await?),
            ctx.identify_service().await?,
            ctx.import_service().await?,
            subscription_repo,
        ),
        notify_service,
        share_resolver: ctx.share_resolver(),
        bot: bot.clone(),
    };

    // Emby proxy server
    let emby_proxy_router = emby_proxy_config.as_ref().map(|config| {
        http::emby_proxy::new_router(
            http::emby_proxy::EmbyProxyContext::new(
                config.upstream_base_url.clone(),
                config.api_key.clone(),
                config.advertise_base_url.clone(),
                config.strm_path_prefix.clone(),
                MediaDownloadUrlService::new(cache.clone(), library_gateway.clone()),
            )
            .expect("validated emby proxy config"),
        )
    });
    let emby_proxy_addr = emby_proxy_config.map(|config| config.addr);

    // Console
    let (console_addr, console_router) = if config.get_console_config().is_enabled() {
        let repo = ctx.import_record_repository().await?;
        let file_index_service = ctx.file_index_service().await?;
        let import_service = ctx.import_service().await?;
        let identify_service = ctx.identify_service().await?;
        let (subscription_service, subscription_repo) = ctx.subscription_service().await?;
        (
            Some(config.get_console_config().get_addr()),
            Some(console::new_router(ConsoleContext::new(
                repo,
                file_index_service,
                import_service,
                identify_service,
                subscription_service,
                subscription_repo,
                delete_media_service,
                std::sync::Arc::new(ctx.community_catalog()),
                std::sync::Arc::new(ctx.share_resolver()),
                LibrarySyncController::new(SyncStrmService::new(
                    library_gateway.clone(),
                    TokioFileStore,
                    library.clone(),
                    library_update_notifier.clone(),
                )),
            ))),
        )
    } else {
        (None, None)
    };

    // Concurrent run
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let shutdown_tx = std::sync::Arc::new(shutdown_tx);

    let shutdown_tx_trigger = shutdown_tx.clone();
    tokio::spawn(async move {
        shutdown_trigger.await;
        let _ = shutdown_tx_trigger.send(true);
    });

    let shutdown_rx_telegram = shutdown_rx.clone();
    let mut shutdown_rx_event = shutdown_rx.clone();
    let mut shutdown_rx_cache = shutdown_rx.clone();

    let event_bus_for_delivery = event_bus.clone();
    let shutdown_tx_servers = shutdown_tx.clone();
    let (server_result, event_result, telegram_result, _) = tokio::join!(
        // Server: HTTP servers (Media server + optional Emby proxy + optional Console)
        async {
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(http::run(
                "media server",
                media_server_addr,
                media_server,
                shutdown_rx.clone(),
            ));
            if let (Some(addr), Some(router)) = (emby_proxy_addr, emby_proxy_router) {
                tasks.spawn(http::run("emby proxy", addr, router, shutdown_rx.clone()));
            }
            if let (Some(addr), Some(router)) = (console_addr, console_router) {
                tasks.spawn(http::run("console", addr, router, shutdown_rx.clone()));
            }
            let res = match tasks.join_next().await {
                Some(Ok(result)) => {
                    tasks.abort_all();
                    result
                }
                Some(Err(err)) => {
                    tasks.abort_all();
                    Err(AppError::Internal(format!("server task failed: {err}")))
                }
                None => Ok(()),
            };
            let _ = shutdown_tx_servers.send(true);
            res
        },
        // Event delivery
        async move {
            event_bus_for_delivery
                .subscribe(
                    event_delivery_telegram,
                    crate::interface::telegram::delivery::on_send_telegram_message,
                )
                .await?;
            event_bus_for_delivery
                .subscribe(event_delivery_media_handler, on_process_media_sources)
                .await?;
            info!("Event bus is running");
            let _ = shutdown_rx_event.wait_for(|&s| s).await;
            info!("Shutting down event bus...");
            Ok::<(), AppError>(())
        },
        // Telegram bot runtime (degraded background task with automatic retry)
        async move {
            telegram::run(bot, bot_runtime, shutdown_rx_telegram).await;
            Ok::<(), AppError>(())
        },
        // Cache cleanup
        async move {
            info!("Cache cleanup task started (interval: 12 hours)");
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_hours(12)) => {
                        match cache.clear_expired().await {
                            Ok(count) => info!("Cache cleanup completed: removed {} expired entries", count),
                            Err(e) => error!("Cache cleanup failed: {}", e),
                        }
                    }
                    _ = async { let _ = shutdown_rx_cache.wait_for(|&s| s).await; } => {
                        info!("Shutting down cache cleanup task...");
                        break;
                    }
                }
            }
        }
    );
    server_result?;
    event_result?;
    telegram_result?;
    Ok(())
}

#[derive(Clone)]
struct EmbyProxyConfig {
    addr: String,
    upstream_base_url: String,
    api_key: Option<String>,
    advertise_base_url: String,
    strm_path_prefix: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        net::TcpListener,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempServerDataDir {
        path: PathBuf,
    }

    impl TempServerDataDir {
        fn new() -> Self {
            let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bigbrother-server-test-{}-{counter}",
                std::process::id()
            ));
            fs::create_dir_all(path.join("config")).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_config(&self, config: &str) {
            fs::write(self.path.join("config/config.yaml"), config).unwrap();
        }
    }

    impl Drop for TempServerDataDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn free_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    #[tokio::test]
    async fn server_starts_http_and_degrades_when_telegram_network_fails() {
        let data_dir = TempServerDataDir::new();
        let media_port = free_port();

        data_dir.write_config(&format!(
            r#"
telegram:
  bot_token: "123456:dummy_token"
  user_id: 123
  api_url: "http://127.0.0.1:1/"
media_server:
  host: "127.0.0.1"
  port: {media_port}
  strm_path_prefix: "/strm"
library:
  remote_path: "/remote"
  local_path: "/local"
"#
        ));

        let (trigger_tx, trigger_rx) = tokio::sync::oneshot::channel::<()>();
        let data_dir_path = data_dir.path().to_str().unwrap().to_string();

        let server_task = tokio::spawn(async move {
            run_with_shutdown(&data_dir_path, async move {
                let _ = trigger_rx.await;
            })
            .await
        });

        // Poll HTTP media server until ready (proves HTTP started and is not blocked by Telegram)
        let media_addr = format!("127.0.0.1:{media_port}");
        let mut connected = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if tokio::net::TcpStream::connect(&media_addr).await.is_ok() {
                connected = true;
                break;
            }
        }
        assert!(
            connected,
            "Media server HTTP interface should be accessible even if Telegram fails"
        );

        // Verify that HTTP server responds to requests
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{media_addr}/unknown-path"))
            .send()
            .await;
        assert!(resp.is_ok(), "HTTP server should respond to requests");

        // Verify server task is still alive (degraded mode, not aborted)
        assert!(
            !server_task.is_finished(),
            "Server task should still be running in degraded mode"
        );

        // Trigger graceful shutdown
        trigger_tx.send(()).unwrap();

        // The server should shut down cleanly within 1 second without hanging
        let join_res = tokio::time::timeout(Duration::from_secs(1), server_task)
            .await
            .expect("Server task should shut down cleanly without hanging");

        let app_res = join_res.expect("Server task should join without panic");
        assert!(app_res.is_ok(), "Server should exit cleanly with Ok(())");
    }

    #[tokio::test]
    async fn server_fails_fast_when_http_port_is_in_use() {
        let occupied_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_port = occupied_listener.local_addr().unwrap().port();

        let data_dir = TempServerDataDir::new();
        data_dir.write_config(&format!(
            r#"
telegram:
  bot_token: "123456:dummy_token"
  user_id: 123
  api_url: "http://127.0.0.1:1/"
media_server:
  host: "127.0.0.1"
  port: {occupied_port}
  strm_path_prefix: "/strm"
library:
  remote_path: "/remote"
  local_path: "/local"
"#
        ));

        let (_trigger_tx, trigger_rx) = tokio::sync::oneshot::channel::<()>();
        let data_dir_path = data_dir.path().to_str().unwrap().to_string();

        let server_task = tokio::spawn(async move {
            run_with_shutdown(&data_dir_path, async move {
                let _ = trigger_rx.await;
            })
            .await
        });

        // The server should fail fast because port is already bound, without hanging in tokio::join!
        let join_res = tokio::time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("Server should fail fast when port is occupied and not hang");

        let app_res = join_res.expect("Server task should join without panic");
        assert!(
            app_res.is_err(),
            "Server should return Err on port conflict"
        );

        drop(occupied_listener);
    }

    #[tokio::test]
    async fn server_fails_fast_when_telegram_api_url_is_invalid() {
        let data_dir = TempServerDataDir::new();
        data_dir.write_config(
            r#"
telegram:
  bot_token: "123456:dummy_token"
  user_id: 123
  api_url: "localhost:8081"
media_server:
  host: "127.0.0.1"
  port: 0
  strm_path_prefix: "/strm"
library:
  remote_path: "/remote"
  local_path: "/local"
"#,
        );

        let (_trigger_tx, trigger_rx) = tokio::sync::oneshot::channel::<()>();
        let data_dir_path = data_dir.path().to_str().unwrap().to_string();

        let server_task = tokio::spawn(async move {
            run_with_shutdown(&data_dir_path, async move {
                let _ = trigger_rx.await;
            })
            .await
        });

        let join_res = tokio::time::timeout(Duration::from_secs(5), server_task)
            .await
            .expect("Server should fail fast on invalid telegram api_url and not hang");

        let app_res = join_res.expect("Server task should join without panic");
        match app_res {
            Err(AppError::InvalidParameter(msg)) => {
                assert!(msg.contains("invalid telegram api_url"));
                assert!(msg.contains("localhost:8081"));
            }
            other => panic!("expected InvalidParameter error, got: {other:?}"),
        }
    }
}
