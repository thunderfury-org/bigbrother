use tracing::warn;

use crate::{
    application::{file_index::SeenFile, import::MetadataLookup},
    domain::share::RawFile,
    error::AppResult,
    infrastructure::services::{
        FileIndexRuntimeService, ImportService, KeywordService, NotifyService,
        ShareResolverRuntimeService,
    },
    infrastructure::share::{file_parser::ShareFileParser, resolver::ShareResolver},
    interface::telegram::file_index::{
        MediaSource, ProcessMediaSources, send_import_error, send_import_results,
    },
};

#[derive(Clone)]
pub struct ProcessMediaSourcesHandler {
    pub file_index_service: FileIndexRuntimeService,
    pub share_resolver: ShareResolverRuntimeService,
    pub import_service: ImportService,
    pub metadata_lookup: MetadataLookup,
    pub notify_service: NotifyService,
    pub keyword_service: KeywordService,
    pub bot: teloxide::Bot,
}

pub async fn on_process_media_sources(
    mut handler: ProcessMediaSourcesHandler,
    payload: ProcessMediaSources,
) -> AppResult<()> {
    let reply_to = payload.reply_to_message_id;
    let description = payload.description.clone();
    let error_prefix = match &payload.source {
        MediaSource::ShareUrl(_) => "分享处理失败",
        MediaSource::Fslink(_) => "秒传处理失败",
        MediaSource::TgDocument { .. } => "JSON/CAS 文件处理失败",
    };

    // Step 1: Fetch raw files (source-specific)
    let raw_files = fetch_raw_files(&handler, &payload.source, reply_to, error_prefix).await?;
    let Some(raw_files) = raw_files else {
        return Ok(());
    };

    // Step 2: Index
    let seen: Vec<SeenFile> = raw_files.iter().map(SeenFile::from_raw_file).collect();
    if let Err(err) = handler
        .file_index_service
        .record_seen_files(seen, description)
        .await
    {
        warn!(error = %err, "file index record failed (non-blocking)");
    }

    // Step 3: Import
    if should_import(
        &handler.keyword_service,
        payload.channel_post,
        &payload.description,
    )
    .await
    {
        let media_files = handler.metadata_lookup.build_media_files(raw_files);
        match handler
            .import_service
            .transfer_media_files(&media_files)
            .await
        {
            Ok(imported) => {
                send_import_results(&handler.notify_service, reply_to, &imported).await;
            }
            Err(err) if !err.is_retryable() => {
                send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
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
    reply_to: Option<i32>,
    error_prefix: &str,
) -> AppResult<Option<Vec<RawFile>>> {
    let result = match source {
        MediaSource::ShareUrl(url) => {
            resolve_share_url_raw_files(&handler.share_resolver, url).await
        }
        MediaSource::Fslink(fslink) => ShareFileParser::parse_fslink(fslink),
        MediaSource::TgDocument { file_id, file_name } => {
            return fetch_tg_document(handler, file_id, file_name, reply_to, error_prefix).await;
        }
    };

    match result {
        Ok(files) => Ok(Some(files)),
        Err(err) if !err.is_retryable() => {
            warn!(error = %err, "skipping permanent error");
            send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
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
        send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
        return Ok(None);
    }

    let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
    handler.bot.download_file(&file.path, &mut content).await?;

    match ShareFileParser::parse_json_bytes(content) {
        Ok(files) => Ok(Some(files)),
        Err(err) => {
            warn!(file_name = %file_name, error = %err, "failed to parse document");
            send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
            Ok(None)
        }
    }
}

async fn should_import(
    keyword_service: &KeywordService,
    channel_post: bool,
    description: &Option<String>,
) -> bool {
    if !channel_post {
        return true;
    }

    let text = description.as_deref().unwrap_or_default();
    keyword_service.matches_any_keyword(text).await
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

        let err = resolve_share_url_raw_files(
            &FailingShareResolver,
            "https://pan.quark.cn/s/share-id?pwd=bad",
        )
        .await
        .unwrap_err();

        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("share password invalid"));
    }
}
