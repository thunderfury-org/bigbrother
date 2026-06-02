use tracing::{info, warn};

use crate::{
    application::import::MetadataLookup,
    application::recorded_import::RecordedImportService,
    application::subscription::import_filter,
    domain::import_record::ImportSource,
    domain::share::RawFile,
    error::AppResult,
    infrastructure::repo::import_record::SeaOrmImportRecordRepository,
    infrastructure::services::{
        FileIndexRuntimeService, IdentifyService, ImportService, NotifyService,
        ShareResolverRuntimeService, SubscriptionRepo,
    },
    infrastructure::share::{file_parser::ShareFileParser, resolver::ShareResolver},
    interface::import::{source_for_fslink, source_for_share_url, source_for_telegram_document},
    interface::telegram::file_index::{
        MediaSource, ProcessMediaSources, send_import_error, send_import_results,
    },
};

#[derive(Clone)]
pub struct ProcessMediaSourcesHandler {
    pub file_index_service: FileIndexRuntimeService,
    pub share_resolver: ShareResolverRuntimeService,
    pub import_service: ImportService,
    pub identify_service: IdentifyService,
    pub recorded_import: RecordedImportService<SeaOrmImportRecordRepository>,
    pub metadata_lookup: MetadataLookup,
    pub notify_service: NotifyService,
    pub subscription_repo: SubscriptionRepo,
    pub bot: teloxide::Bot,
}

fn source_of(source: &MediaSource) -> ImportSource {
    match source {
        MediaSource::ShareUrl(url) => source_for_share_url(url),
        MediaSource::Fslink(raw) => source_for_fslink(raw),
        MediaSource::TgDocument { file_name, .. } => source_for_telegram_document(file_name),
    }
}

pub async fn on_process_media_sources(
    mut handler: ProcessMediaSourcesHandler,
    payload: ProcessMediaSources,
) -> AppResult<()> {
    let reply_to = payload.source_reply_to_message_id();
    let description = payload.description.clone();
    let source_context = payload.source_context();
    let error_prefix = match &payload.source {
        MediaSource::ShareUrl(_) => "分享处理失败",
        MediaSource::Fslink(_) => "秒传处理失败",
        MediaSource::TgDocument { .. } => "JSON/CAS 文件处理失败",
    };
    info!(
        source_kind = payload.source.kind(),
        channel_post = payload.source_channel_post(),
        reply_to = ?reply_to,
        source_chat_id = ?payload.source_chat_id(),
        source_message_id = ?payload.source_message_id(),
        source_message_link = ?payload.source_message_link(),
        "Processing media source observation"
    );

    // Step 1: Fetch raw files (source-specific)
    let raw_files = fetch_raw_files(
        &handler,
        &payload.source,
        source_context,
        reply_to,
        error_prefix,
    )
    .await?;
    let Some(raw_files) = raw_files else {
        info!(
            source_kind = payload.source.kind(),
            source_chat_id = ?payload.source_chat_id(),
            source_message_id = ?payload.source_message_id(),
            "Media source observation ended without raw files"
        );
        return Ok(());
    };
    info!(
        source_kind = payload.source.kind(),
        raw_file_count = raw_files.len(),
        source_chat_id = ?payload.source_chat_id(),
        source_message_id = ?payload.source_message_id(),
        "Fetched raw files for media source observation"
    );

    // Step 2: Index
    if let Err(err) = handler
        .file_index_service
        .record_raw_files(raw_files.clone(), description.clone())
        .await
    {
        warn!(error = %err, "file index record failed (non-blocking)");
    } else {
        info!(
            source_kind = payload.source.kind(),
            raw_file_count = raw_files.len(),
            source_chat_id = ?payload.source_chat_id(),
            source_message_id = ?payload.source_message_id(),
            "Recorded raw files into file index"
        );
    }

    // Step 3: Import
    let should_import = should_import(
        &handler.subscription_repo,
        payload.source_channel_post(),
        &payload.description,
    )
    .await;
    info!(
        source_kind = payload.source.kind(),
        should_import,
        channel_post = payload.source_channel_post(),
        source_chat_id = ?payload.source_chat_id(),
        source_message_id = ?payload.source_message_id(),
        "Evaluated import policy for media source observation"
    );
    if should_import {
        let descriptions: Vec<String> = description.into_iter().collect();
        let media_files = handler
            .metadata_lookup
            .build_media_files(raw_files, descriptions);
        info!(
            source_kind = payload.source.kind(),
            media_file_count = media_files.len(),
            source_chat_id = ?payload.source_chat_id(),
            source_message_id = ?payload.source_message_id(),
            "Built media files for import"
        );
        let import_source = source_of(&payload.source);
        let is_channel_post = payload.source_channel_post();
        let mut import_service = handler.import_service.clone();
        let mut identify_service = handler.identify_service.clone();
        let subscription_repo = handler.subscription_repo.clone();
        let outcome = handler
            .recorded_import
            .execute(import_source, move || async move {
                let outcome = identify_service.identify(media_files).await?;
                let groups = if is_channel_post {
                    import_filter::filter_by_subscription(&subscription_repo, outcome.groups).await
                } else {
                    outcome.groups
                };
                import_service
                    .import_groups(groups, outcome.unmatched)
                    .await
            })
            .await;
        match outcome {
            Ok(imported) => {
                info!(
                    source_kind = payload.source.kind(),
                    imported_summary_count = imported.len(),
                    source_chat_id = ?payload.source_chat_id(),
                    source_message_id = ?payload.source_message_id(),
                    "Import completed for media source observation"
                );
                send_import_results(&handler.notify_service, source_context, reply_to, &imported)
                    .await;
            }
            Err(err) if !err.is_retryable() => {
                send_import_error(
                    &handler.notify_service,
                    source_context,
                    reply_to,
                    error_prefix,
                    &err,
                )
                .await;
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

/// Fetch raw files from the source. Returns Ok(None) if the source should be skipped (permanent error).
async fn fetch_raw_files(
    handler: &ProcessMediaSourcesHandler,
    source: &MediaSource,
    source_context: Option<&crate::interface::telegram::file_index::SourceContext>,
    reply_to: Option<i32>,
    error_prefix: &str,
) -> AppResult<Option<Vec<RawFile>>> {
    let result = match source {
        MediaSource::ShareUrl(url) => {
            resolve_share_url_raw_files(&handler.share_resolver, url).await
        }
        MediaSource::Fslink(fslink) => ShareFileParser::parse_fslink(fslink),
        MediaSource::TgDocument { file_id, file_name } => {
            return fetch_tg_document(
                handler,
                file_id,
                file_name,
                source_context,
                reply_to,
                error_prefix,
            )
            .await;
        }
    };

    match result {
        Ok(files) => Ok(Some(files)),
        Err(err) if !err.is_retryable() => {
            warn!(error = %err, "skipping permanent error");
            send_import_error(
                &handler.notify_service,
                source_context,
                reply_to,
                error_prefix,
                &err,
            )
            .await;
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn resolve_share_url_raw_files<R: ShareResolver>(
    resolver: &R,
    raw_url: &str,
) -> AppResult<Vec<RawFile>> {
    resolver.raw_files_from_url(raw_url).await?.ok_or_else(|| {
        crate::error::AppError::InvalidParameter(format!("unsupported share url: {raw_url}"))
    })
}

async fn fetch_tg_document(
    handler: &ProcessMediaSourcesHandler,
    file_id: &str,
    file_name: &str,
    source_context: Option<&crate::interface::telegram::file_index::SourceContext>,
    reply_to: Option<i32>,
    error_prefix: &str,
) -> AppResult<Option<Vec<RawFile>>> {
    use teloxide::net::Download;
    use teloxide::prelude::Requester;

    let file = handler
        .bot
        .get_file(teloxide::types::FileId(file_id.to_string()))
        .await?;

    if file.meta.size > 10 * 1024 * 1024 {
        let err = crate::error::AppError::InvalidParameter(format!(
            "Telegram document too large ({file_name}): {} bytes exceeds 10MB limit",
            file.meta.size
        ));
        warn!(file_name = %file_name, size = file.meta.size, "document too large");
        send_import_error(
            &handler.notify_service,
            source_context,
            reply_to,
            error_prefix,
            &err,
        )
        .await;
        return Ok(None);
    }

    let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
    handler.bot.download_file(&file.path, &mut content).await?;

    match ShareFileParser::parse_json_bytes(content) {
        Ok(files) => Ok(Some(files)),
        Err(err) => {
            warn!(file_name = %file_name, error = %err, "failed to parse document");
            send_import_error(
                &handler.notify_service,
                source_context,
                reply_to,
                error_prefix,
                &err,
            )
            .await;
            Ok(None)
        }
    }
}

async fn should_import(
    subscription_repo: &SubscriptionRepo,
    channel_post: bool,
    description: &Option<String>,
) -> bool {
    if !channel_post {
        return true;
    }
    let text = description.as_deref().unwrap_or_default();
    import_filter::description_matches_subscription(subscription_repo, text).await
}

#[cfg(test)]
mod tests {
    use super::resolve_share_url_raw_files;
    use crate::{
        domain::share::RawFile,
        error::{AppError, AppResult},
        infrastructure::share::resolver::ShareResolver,
    };

    #[derive(Clone)]
    struct FakeShareResolver {
        result: Option<Vec<RawFile>>,
    }

    impl ShareResolver for FakeShareResolver {
        async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn resolve_share_url_raw_files_rejects_unsupported_provider() {
        let err = resolve_share_url_raw_files(
            &FakeShareResolver { result: None },
            "https://example.com/share",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("unsupported share url"));
    }

    #[tokio::test]
    async fn resolve_share_url_raw_files_keeps_supported_provider_failures_visible() {
        #[derive(Clone)]
        struct FailingShareResolver;

        impl ShareResolver for FailingShareResolver {
            async fn raw_files_from_url(&self, _url: &str) -> AppResult<Option<Vec<RawFile>>> {
                Err(AppError::InvalidParameter(
                    "share password invalid".to_string(),
                ))
            }
        }

        let err =
            resolve_share_url_raw_files(&FailingShareResolver, "https://115.com/s/share-id?rc=bad")
                .await
                .unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("share password invalid"));
    }
}
