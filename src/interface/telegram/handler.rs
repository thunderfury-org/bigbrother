use tracing::{info, warn};

use crate::{
    application::media_source_observation::{MediaSourceObservation, ObservationNotice},
    application::ports::MessageSender,
    domain::import_record::ImportSource,
    domain::share::RawFile,
    error::{AppError, AppResult},
    infrastructure::share::file_parser::ShareFileParser,
    interface::import::{source_for_fslink, source_for_share_url, source_for_telegram_document},
    interface::runtime::{NotifyService, ObservationProcessor, ShareResolverRuntimeService},
    interface::telegram::file_index::{
        MediaSource, ProcessMediaSources, send_import_error, send_import_results,
    },
};

const MAX_TELEGRAM_DOCUMENT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone)]
pub struct ProcessMediaSourcesHandler {
    pub processor: ObservationProcessor,
    pub notify_service: NotifyService,
    pub share_resolver: ShareResolverRuntimeService,
    pub bot: teloxide::Bot,
}

pub async fn on_process_media_sources(
    handler: ProcessMediaSourcesHandler,
    payload: ProcessMediaSources,
) -> AppResult<()> {
    let reply_to = payload.source_reply_to_message_id();
    let source_context = payload.source_context().cloned();
    let error_prefix = error_prefix(&payload.source);
    info!(
        source_kind = payload.source.kind(),
        channel_post = payload.source_channel_post(),
        reply_to = ?reply_to,
        source_chat_id = ?payload.source_chat_id(),
        source_message_id = ?payload.source_message_id(),
        source_message_link = ?payload.source_message_link(),
        "Processing media source observation"
    );

    let raw_files =
        match fetch_raw_files(&handler.share_resolver, &handler.bot, &payload.source).await {
            Ok(files) => files,
            Err(err) if !err.is_retryable() => {
                warn!(error = %err, "skipping permanent error");
                send_import_error(
                    &handler.notify_service,
                    source_context.as_ref(),
                    reply_to,
                    error_prefix,
                    &err,
                )
                .await;
                return Ok(());
            }
            Err(err) => return Err(err),
        };
    info!(
        source_kind = payload.source.kind(),
        raw_file_count = raw_files.len(),
        "Fetched raw files for media source observation"
    );

    let notice = handler
        .processor
        .process(observation_from_payload(&payload, raw_files))
        .await?;
    deliver_observation_notice(
        &handler.notify_service,
        source_context.as_ref(),
        reply_to,
        error_prefix,
        notice,
    )
    .await;
    Ok(())
}

fn observation_from_payload(
    payload: &ProcessMediaSources,
    raw_files: Vec<RawFile>,
) -> MediaSourceObservation {
    MediaSourceObservation {
        import_source: import_source_of(&payload.source),
        description: payload.description.clone(),
        channel_post: payload.source_channel_post(),
        raw_files,
    }
}

fn import_source_of(source: &MediaSource) -> ImportSource {
    match source {
        MediaSource::ShareUrl(url) => source_for_share_url(url),
        MediaSource::Fslink(raw) => source_for_fslink(raw),
        MediaSource::TgDocument { file_name, .. } => source_for_telegram_document(file_name),
    }
}

fn error_prefix(source: &MediaSource) -> &'static str {
    match source {
        MediaSource::ShareUrl(_) => "分享处理失败",
        MediaSource::Fslink(_) => "秒传处理失败",
        MediaSource::TgDocument { .. } => "JSON/CAS 文件处理失败",
    }
}

async fn fetch_raw_files(
    resolver: &dyn crate::application::ports::ShareResolver,
    bot: &teloxide::Bot,
    source: &MediaSource,
) -> AppResult<Vec<RawFile>> {
    match source {
        MediaSource::ShareUrl(url) => resolve_share_url_raw_files(resolver, url).await,
        MediaSource::Fslink(fslink) => ShareFileParser::parse_fslink(fslink),
        MediaSource::TgDocument { file_id, file_name } => {
            fetch_tg_document(bot, file_id, file_name).await
        }
    }
}

async fn resolve_share_url_raw_files(
    resolver: &dyn crate::application::ports::ShareResolver,
    raw_url: &str,
) -> AppResult<Vec<RawFile>> {
    resolver
        .raw_files_from_url(raw_url)
        .await?
        .ok_or_else(|| AppError::InvalidParameter(format!("unsupported share url: {raw_url}")))
}

async fn fetch_tg_document(
    bot: &teloxide::Bot,
    file_id: &str,
    file_name: &str,
) -> AppResult<Vec<RawFile>> {
    use teloxide::net::Download;
    use teloxide::prelude::Requester;

    let file = bot
        .get_file(teloxide::types::FileId(file_id.to_string()))
        .await?;
    let size = u64::from(file.meta.size);
    if size > MAX_TELEGRAM_DOCUMENT_BYTES {
        return Err(AppError::InvalidParameter(format!(
            "Telegram document too large ({file_name}): {size} bytes exceeds 10MB limit"
        )));
    }

    let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
    bot.download_file(&file.path, &mut content).await?;
    ShareFileParser::parse_json_bytes(content)
}

async fn deliver_observation_notice(
    notify_service: &impl MessageSender,
    source_context: Option<&crate::interface::telegram::file_index::SourceContext>,
    reply_to: Option<i32>,
    error_prefix: &str,
    notice: Option<ObservationNotice>,
) {
    match notice {
        Some(ObservationNotice::ImportResults(imported)) => {
            send_import_results(notify_service, source_context, reply_to, &imported).await;
        }
        Some(ObservationNotice::PermanentError { error }) => {
            send_import_error(
                notify_service,
                source_context,
                reply_to,
                error_prefix,
                &error,
            )
            .await;
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        deliver_observation_notice, error_prefix, observation_from_payload,
        resolve_share_url_raw_files,
    };
    use crate::application::import::ImportedMedia;
    use crate::application::media_source_observation::ObservationNotice;
    use crate::application::ports::ShareResolver;
    use crate::application::ports::{Message, MessageSender};
    use crate::domain::import_record::ImportSourceKind;
    use crate::domain::share::RawFile;
    use crate::error::{AppError, AppResult};
    use crate::interface::telegram::file_index::{
        MediaSource, ProcessMediaSources, SourceContext, TelegramSourceContext,
    };
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone)]
    struct FakeShareResolver {
        result: AppResult<Option<Vec<RawFile>>>,
    }

    #[async_trait::async_trait]
    impl ShareResolver for FakeShareResolver {
        async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
            self.result.clone()
        }
    }

    #[derive(Clone, Default)]
    struct FakeSender {
        payloads: Arc<Mutex<Vec<Message>>>,
    }

    impl MessageSender for FakeSender {
        async fn send(&self, payload: &Message) -> AppResult<()> {
            self.payloads.lock().unwrap().push(payload.clone());
            Ok(())
        }
    }

    fn telegram_context(channel_post: bool, reply_to: Option<i32>) -> SourceContext {
        SourceContext::Telegram(TelegramSourceContext {
            channel_post,
            reply_to_message_id: reply_to,
            source_chat_id: -1001234567890,
            source_message_id: 321,
            source_message_link: Some("https://t.me/c/1234567890/321".into()),
        })
    }

    fn sample_raw_file() -> RawFile {
        RawFile {
            id: None,
            name: "Inception.2010.1080p.mkv".into(),
            hash: crate::domain::share::FileHash::Md5("a".repeat(32)),
            size: 1000,
            path: "/share".into(),
        }
    }

    #[test]
    fn observation_from_payload_prefers_source_context_and_keeps_import_source() {
        let payload = ProcessMediaSources {
            source: MediaSource::ShareUrl("https://115.com/s/share-id?rc=abc".into()),
            description: Some("Inception".into()),
            source_context: Some(telegram_context(true, None)),
            channel_post: false,
            reply_to_message_id: Some(7),
        };

        let observation = observation_from_payload(&payload, vec![sample_raw_file()]);
        assert_eq!(observation.import_source.kind, ImportSourceKind::Pan115);
        assert_eq!(
            observation.import_source.raw,
            "https://115.com/s/share-id?rc=abc"
        );
        assert_eq!(observation.description.as_deref(), Some("Inception"));
        assert!(observation.channel_post);
        assert_eq!(observation.raw_files.len(), 1);
    }

    #[test]
    fn observation_from_payload_maps_telegram_document_source() {
        let payload = ProcessMediaSources {
            source: MediaSource::TgDocument {
                file_id: "file-1".into(),
                file_name: "dump.json".into(),
            },
            description: None,
            source_context: None,
            channel_post: false,
            reply_to_message_id: Some(8),
        };

        let observation = observation_from_payload(&payload, Vec::new());
        assert_eq!(observation.import_source.kind, ImportSourceKind::Telegram);
        assert_eq!(observation.import_source.raw, "dump.json");
        assert!(!observation.channel_post);
    }

    #[test]
    fn error_prefix_matches_source_kind() {
        assert_eq!(
            error_prefix(&MediaSource::ShareUrl("https://115.com/s/a".into())),
            "分享处理失败"
        );
        assert_eq!(
            error_prefix(&MediaSource::Fslink("123FSLinkV2$x".into())),
            "秒传处理失败"
        );
        assert_eq!(
            error_prefix(&MediaSource::TgDocument {
                file_id: "1".into(),
                file_name: "a.json".into(),
            }),
            "JSON/CAS 文件处理失败"
        );
    }

    #[tokio::test]
    async fn resolve_share_url_raw_files_rejects_unsupported_provider() {
        let err = resolve_share_url_raw_files(
            &FakeShareResolver { result: Ok(None) },
            "https://example.com/share",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("unsupported share url"));
    }

    #[tokio::test]
    async fn resolve_share_url_raw_files_keeps_supported_provider_failures_visible() {
        let err = resolve_share_url_raw_files(
            &FakeShareResolver {
                result: Err(AppError::InvalidParameter("share password invalid".into())),
            },
            "https://115.com/s/share-id?rc=bad",
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("share password invalid"));
    }

    #[tokio::test]
    async fn deliver_import_results_keeps_reply_to_and_source_link() {
        let sender = FakeSender::default();
        let payloads = sender.payloads.clone();

        deliver_observation_notice(
            &sender,
            Some(&telegram_context(true, Some(7))),
            Some(7),
            "分享处理失败",
            Some(ObservationNotice::ImportResults(vec![
                ImportedMedia::Movie {
                    title: "Inception".into(),
                    year: "2010".into(),
                    size: 2 * 1024 * 1024 * 1024,
                    cost: Duration::from_secs(2),
                    has_failed: false,
                },
            ])),
        )
        .await;

        let payloads = payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].reply_to, Some(7));
        assert!(payloads[0].message.contains("Inception"));
        assert!(
            payloads[0]
                .message
                .contains("源消息: https://t.me/c/1234567890/321")
        );
    }

    #[tokio::test]
    async fn deliver_permanent_error_uses_source_prefix_and_transport_context() {
        let sender = FakeSender::default();
        let payloads = sender.payloads.clone();

        deliver_observation_notice(
            &sender,
            Some(&telegram_context(true, None)),
            None,
            "秒传处理失败",
            Some(ObservationNotice::PermanentError {
                error: AppError::InvalidParameter("bad fslink".into()),
            }),
        )
        .await;

        let payloads = payloads.lock().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].reply_to, None);
        assert!(payloads[0].message.starts_with("秒传处理失败: 参数错误："));
        assert!(payloads[0].message.contains("bad fslink"));
        assert!(
            payloads[0]
                .message
                .contains("源消息: https://t.me/c/1234567890/321")
        );
    }

    #[tokio::test]
    async fn deliver_skips_notification_when_outcome_has_no_notice() {
        let sender = FakeSender::default();
        let payloads = sender.payloads.clone();

        deliver_observation_notice(
            &sender,
            Some(&telegram_context(true, Some(7))),
            Some(7),
            "分享处理失败",
            None,
        )
        .await;

        assert!(payloads.lock().unwrap().is_empty());
    }
}
