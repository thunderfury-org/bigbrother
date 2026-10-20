use std::time::Duration;
use teloxide::prelude::*;
use tracing::{error, info, warn};

use crate::{
    application::ports::{Message as OutboundMessage, MessageSender},
    infrastructure::event_bus::EventBus,
    interface::runtime::NotifyService,
};
pub(crate) mod delivery;
pub mod export;
pub mod file_index;
pub(crate) mod handler;

const NO_VALID_MEDIA_SOURCE_MESSAGE: &str =
    "未发现有效分享链接，仅支持 Pan123、天翼、115 分享链接，或 fslink、.json/.cas 文件";

#[derive(Debug, PartialEq, Eq)]
enum SourceHandling {
    Ignore,
    NotifyNoValidMediaSource,
    Process { confirm: String },
}

#[derive(Clone)]
pub(crate) struct BotRuntime {
    pub user_id: UserId,
    notify: NotifyService,
    event_bus: EventBus,
}

pub(crate) struct BotRuntimeArgs {
    pub user_id: UserId,
    pub notify_service: NotifyService,
    pub event_bus: EventBus,
}

impl BotRuntime {
    pub(crate) fn new(args: BotRuntimeArgs) -> Self {
        Self {
            user_id: args.user_id,
            notify: args.notify_service,
            event_bus: args.event_bus,
        }
    }

    fn notify_service(&self) -> &NotifyService {
        &self.notify
    }

    fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

pub(crate) async fn run(
    bot: teloxide::Bot,
    runtime: BotRuntime,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    run_with_retry(bot, runtime, shutdown_rx, Duration::from_secs(5)).await;
}

pub(crate) async fn run_with_retry(
    bot: teloxide::Bot,
    runtime: BotRuntime,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    initial_retry_delay: Duration,
) {
    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(Update::filter_message().endpoint(handle_message));

    let mut backoff = initial_retry_delay;
    let max_backoff = Duration::from_secs(60);

    loop {
        if *shutdown_rx.borrow() {
            info!("Telegram bot shutdown requested before starting");
            break;
        }

        info!("Starting Telegram bot dispatcher...");

        let bot_for_task = bot.clone();
        let handler_for_task = handler.clone();
        let runtime_for_task = runtime.clone();

        let mut dispatcher = Dispatcher::builder(bot_for_task.clone(), handler_for_task)
            .dependencies(dptree::deps![runtime_for_task])
            .build();

        let shutdown_token = dispatcher.shutdown_token();
        let mut shutdown_watch = shutdown_rx.clone();

        let mut dispatch_handle = tokio::spawn(async move {
            let listener = teloxide::update_listeners::polling_default(bot_for_task).await;
            let error_handler = teloxide::error_handlers::LoggingErrorHandler::with_custom_text(
                "An error from the update listener",
            );
            dispatcher
                .try_dispatch_with_listener(listener, error_handler)
                .await
        });

        tokio::select! {
            join_res = &mut dispatch_handle => {
                match join_res {
                    Ok(Ok(())) => {
                        info!("Telegram bot dispatcher stopped gracefully");
                        break;
                    }
                    Ok(Err(err)) => {
                        if *shutdown_rx.borrow() {
                            info!("Telegram bot stopping due to shutdown signal");
                            break;
                        }
                        warn!(
                            "Telegram bot initialization failed: {err}. Retrying in {:?} in background...",
                            backoff
                        );
                        tokio::select! {
                            _ = tokio::time::sleep(backoff) => {}
                            _ = async { let _ = shutdown_rx.wait_for(|&s| s).await; } => {
                                info!("Telegram bot retry cancelled by shutdown signal");
                                break;
                            }
                        }
                        backoff = (backoff * 2).min(max_backoff);
                    }
                    Err(join_err) => {
                        if *shutdown_rx.borrow() || join_err.is_cancelled() {
                            info!("Telegram bot stopping due to shutdown signal");
                            break;
                        }
                        error!("Telegram bot task failed unexpectedly: {join_err}");
                        tokio::select! {
                            _ = tokio::time::sleep(backoff) => {}
                            _ = async { let _ = shutdown_rx.wait_for(|&s| s).await; } => {
                                info!("Telegram bot retry cancelled by shutdown signal");
                                break;
                            }
                        }
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }
            }
            _ = async { let _ = shutdown_watch.wait_for(|&s| s).await; } => {
                info!("Shutdown signal received, stopping Telegram bot...");
                match shutdown_token.shutdown() {
                    Ok(shutdown_fut) => {
                        if tokio::time::timeout(Duration::from_secs(5), shutdown_fut)
                            .await
                            .is_err()
                        {
                            warn!("Telegram bot graceful shutdown timed out, aborting task");
                            dispatch_handle.abort();
                        }
                        let _ = dispatch_handle.await;
                    }
                    Err(_) => {
                        dispatch_handle.abort();
                        let _ = dispatch_handle.await;
                    }
                }
                break;
            }
        }
    }
}

async fn handle_channel_post(runtime: BotRuntime, msg: Message) -> ResponseResult<()> {
    let sources = file_index::extract_media_sources(&msg);
    let source_context = file_index::telegram_source_context_from_message(&msg, true, None);
    if matches!(
        decide_source_handling(true, &sources),
        SourceHandling::Ignore
    ) {
        info!(
            source_chat_id = source_context.source_chat_id(),
            source_message_id = source_context.source_message_id(),
            source_message_link = ?source_context.source_message_link(),
            extracted_media_sources = 0,
            "Skipping channel source record without media sources"
        );
        return Ok(());
    }

    let description = file_index::message_description(&msg);
    info!(
        source_chat_id = source_context.source_chat_id(),
        source_message_id = source_context.source_message_id(),
        source_message_link = ?source_context.source_message_link(),
        extracted_media_sources = sources.len(),
        "Received channel source record with media sources"
    );

    for source in sources {
        let event = file_index::ProcessMediaSources {
            source,
            description: description.clone(),
            source_context: Some(source_context.clone()),
            channel_post: true,
            reply_to_message_id: None,
        };
        info!(
            source_kind = event.source.kind(),
            source_chat_id = ?event.source_chat_id(),
            source_message_id = ?event.source_message_id(),
            source_message_link = ?event.source_message_link(),
            "Publishing ProcessMediaSources event"
        );
        if let Err(err) = runtime.event_bus().publish(&event).await {
            error!("Failed to publish ProcessMediaSources event: {err}");
        }
    }

    Ok(())
}

async fn handle_message(runtime: BotRuntime, msg: Message) -> ResponseResult<()> {
    info!("Received message from {:?}", msg.chat);

    if msg.from.as_ref().is_none_or(|u| u.id != runtime.user_id) {
        info!("Ignoring message from unauthorized user: {:?}", msg.from);
        return Ok(());
    }

    let sources = file_index::extract_media_sources(&msg);
    let description = file_index::message_description(&msg);
    let reply_to = msg.from.as_ref().map(|_| msg.id.0);
    let source_context = file_index::telegram_source_context_from_message(&msg, false, reply_to);
    info!(
        source_chat_id = source_context.source_chat_id(),
        source_message_id = source_context.source_message_id(),
        source_message_link = ?source_context.source_message_link(),
        extracted_media_sources = sources.len(),
        "Received private source record"
    );
    let handling = decide_source_handling(false, &sources);

    match &handling {
        SourceHandling::Ignore => return Ok(()),
        SourceHandling::NotifyNoValidMediaSource => {
            if let Err(err) = runtime
                .notify_service()
                .send(&OutboundMessage::new(
                    NO_VALID_MEDIA_SOURCE_MESSAGE,
                    reply_to,
                ))
                .await
            {
                error!("Failed to send no-valid-media-source message: {err}");
            }
            return Ok(());
        }
        SourceHandling::Process { .. } => {}
    }

    let mut published_sources = 0usize;

    for source in sources {
        let event = file_index::ProcessMediaSources {
            source,
            description: description.clone(),
            source_context: Some(source_context.clone()),
            channel_post: false,
            reply_to_message_id: reply_to,
        };
        info!(
            source_kind = event.source.kind(),
            source_chat_id = ?event.source_chat_id(),
            source_message_id = ?event.source_message_id(),
            source_message_link = ?event.source_message_link(),
            "Publishing ProcessMediaSources event"
        );
        match runtime.event_bus().publish(&event).await {
            Ok(()) => {
                published_sources += 1;
            }
            Err(err) => {
                error!("Failed to publish ProcessMediaSources event: {err}");
            }
        }
    }

    if published_sources > 0
        && let SourceHandling::Process { confirm } = handling
    {
        file_index::send_observation_notification(
            runtime.notify_service(),
            Some(&source_context),
            reply_to,
            "import_start",
            confirm,
        )
        .await;
    }

    Ok(())
}

fn decide_source_handling(
    channel_post: bool,
    sources: &[file_index::MediaSource],
) -> SourceHandling {
    if sources.is_empty() {
        return if channel_post {
            SourceHandling::Ignore
        } else {
            SourceHandling::NotifyNoValidMediaSource
        };
    }

    SourceHandling::Process {
        confirm: import_start_message(sources),
    }
}

fn import_start_message(sources: &[file_index::MediaSource]) -> String {
    match sources {
        [file_index::MediaSource::ShareUrl(url)] => format!("开始处理分享: {url}"),
        [file_index::MediaSource::Fslink(_)] => "开始处理秒传".to_owned(),
        [file_index::MediaSource::TgDocument { file_name, .. }] => {
            format!("开始处理文件: {file_name}")
        }
        _ => format!("发现 {} 个有效来源，开始处理", sources.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NO_VALID_MEDIA_SOURCE_MESSAGE, SourceHandling, decide_source_handling, import_start_message,
    };
    use crate::interface::telegram::file_index::{MediaSource, extract_media_sources};
    use serde_json::json;
    use teloxide::types::Message;

    #[test]
    fn import_start_message_is_specific_for_single_share_url() {
        let message = import_start_message(&[MediaSource::ShareUrl(
            "https://115.com/s/share-id?rc=abc".to_string(),
        )]);

        assert_eq!(message, "开始处理分享: https://115.com/s/share-id?rc=abc");
    }

    #[test]
    fn import_start_message_summarizes_multiple_media_sources() {
        let message = import_start_message(&[
            MediaSource::ShareUrl("https://115.com/s/share-id?rc=abc".to_string()),
            MediaSource::Fslink(
                "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv".into(),
            ),
        ]);

        assert_eq!(message, "发现 2 个有效来源，开始处理");
    }

    #[test]
    fn private_message_without_media_sources_replies_once_with_supported_input_hint() {
        assert_eq!(
            decide_source_handling(false, &[]),
            SourceHandling::NotifyNoValidMediaSource
        );
        assert_eq!(
            NO_VALID_MEDIA_SOURCE_MESSAGE,
            "未发现有效分享链接，仅支持 Pan123、天翼、115 分享链接，或 fslink、.json/.cas 文件"
        );
    }

    #[test]
    fn channel_post_without_media_sources_is_silently_ignored() {
        assert_eq!(decide_source_handling(true, &[]), SourceHandling::Ignore);
    }

    fn private_text_message(text: &str) -> Message {
        serde_json::from_value(json!({
            "message_id": 1,
            "date": 1_700_000_000,
            "chat": {
                "id": 42,
                "type": "private"
            },
            "text": text
        }))
        .unwrap()
    }

    #[test]
    fn former_slash_commands_are_treated_as_plain_text_without_media_sources() {
        for text in ["/help", "/delete_media foo"] {
            let sources = extract_media_sources(&private_text_message(text));
            assert!(sources.is_empty(), "{text}");
            assert_eq!(
                decide_source_handling(false, &sources),
                SourceHandling::NotifyNoValidMediaSource,
                "{text}"
            );
        }
    }

    #[test]
    fn mixed_supported_and_unsupported_inputs_only_process_supported_sources() {
        let msg = private_text_message(
            "https://115.com/s/share-id?rc=abc\nhttps://www.themoviedb.org/tv/314784",
        );

        let sources = extract_media_sources(&msg);
        let handling = decide_source_handling(false, &sources);

        assert_eq!(
            sources,
            vec![MediaSource::ShareUrl(
                "https://115.com/s/share-id?rc=abc".to_string()
            )]
        );
        assert_eq!(
            handling,
            SourceHandling::Process {
                confirm: "开始处理分享: https://115.com/s/share-id?rc=abc".to_string()
            }
        );
    }

    #[tokio::test]
    async fn run_does_not_panic_and_retries_on_network_failure() {
        use sea_orm_migration::MigratorTrait;

        let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = sea_orm::Database::connect(options).await.unwrap();
        crate::migration::Migrator::up(&db, None).await.unwrap();
        let event_bus = crate::infrastructure::event_bus::EventBus::new(db);
        let notify_service =
            crate::infrastructure::event::publisher::EventBusPublisher::new(event_bus.clone());
        let runtime = super::BotRuntime::new(super::BotRuntimeArgs {
            user_id: teloxide::types::UserId(123),
            notify_service,
            event_bus,
        });

        let bot = teloxide::Bot::new("123456:dummy_token")
            .set_api_url(reqwest::Url::parse("http://127.0.0.1:1/").unwrap());

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(async move {
            super::run_with_retry(
                bot,
                runtime,
                shutdown_rx,
                std::time::Duration::from_millis(30),
            )
            .await;
        });

        // Let it attempt multiple retries in the background
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify task did not panic or exit prematurely
        assert!(
            !handle.is_finished(),
            "telegram task should keep retrying in background"
        );

        // Now trigger shutdown signal
        shutdown_tx.send(true).unwrap();

        // The task should stop cleanly within 500ms
        let join_res = tokio::time::timeout(std::time::Duration::from_millis(500), handle)
            .await
            .expect("telegram task should stop cleanly on shutdown signal");

        assert!(
            join_res.is_ok(),
            "telegram task should exit cleanly without panic"
        );
    }

    #[tokio::test]
    async fn run_deletes_configured_webhook_before_polling() {
        use sea_orm_migration::MigratorTrait;
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = sea_orm::Database::connect(options).await.unwrap();
        crate::migration::Migrator::up(&db, None).await.unwrap();
        let event_bus = crate::infrastructure::event_bus::EventBus::new(db);
        let notify_service =
            crate::infrastructure::event::publisher::EventBusPublisher::new(event_bus.clone());
        let runtime = super::BotRuntime::new(super::BotRuntimeArgs {
            user_id: teloxide::types::UserId(123),
            notify_service,
            event_bus,
        });

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/bot123456:dummy_token/GetWebhookInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {
                    "url": "https://example.com/webhook",
                    "has_custom_certificate": false,
                    "pending_update_count": 0
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/bot123456:dummy_token/DeleteWebhook"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": true,
                "description": "Webhook was deleted"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/bot123456:dummy_token/GetMe"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {
                    "id": 123456,
                    "is_bot": true,
                    "first_name": "Test Bot",
                    "username": "test_bot",
                    "can_join_groups": true,
                    "can_read_all_group_messages": false,
                    "supports_inline_queries": false,
                    "has_main_web_app": false
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/bot123456:dummy_token/GetUpdates"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": []
            })))
            .mount(&server)
            .await;

        let bot = teloxide::Bot::new("123456:dummy_token")
            .set_api_url(reqwest::Url::parse(&format!("{}/", server.uri())).unwrap());

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(async move {
            super::run_with_retry(
                bot,
                runtime,
                shutdown_rx,
                std::time::Duration::from_millis(30),
            )
            .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        shutdown_tx.send(true).unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), handle).await;

        server.verify().await;
    }

    #[tokio::test]
    async fn run_shutdown_cancels_in_flight_stalled_request_promptly() {
        use sea_orm_migration::MigratorTrait;

        let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = sea_orm::Database::connect(options).await.unwrap();
        crate::migration::Migrator::up(&db, None).await.unwrap();
        let event_bus = crate::infrastructure::event_bus::EventBus::new(db);
        let notify_service =
            crate::infrastructure::event::publisher::EventBusPublisher::new(event_bus.clone());
        let runtime = super::BotRuntime::new(super::BotRuntimeArgs {
            user_id: teloxide::types::UserId(123),
            notify_service,
            event_bus,
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let stall_server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 1024];
                    while let Ok(n) = socket.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                    }
                });
            }
        });

        let bot = teloxide::Bot::new("123456:dummy_token")
            .set_api_url(reqwest::Url::parse(&format!("http://{addr}/")).unwrap());

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(async move {
            super::run_with_retry(
                bot,
                runtime,
                shutdown_rx,
                std::time::Duration::from_millis(30),
            )
            .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !handle.is_finished(),
            "telegram task should be in-flight/stalled"
        );

        shutdown_tx.send(true).unwrap();

        let join_res = tokio::time::timeout(std::time::Duration::from_millis(500), handle)
            .await
            .expect("telegram task should cancel in-flight stalled request promptly on shutdown");

        assert!(join_res.is_ok(), "telegram task should exit cleanly");
        stall_server.abort();
    }
}
