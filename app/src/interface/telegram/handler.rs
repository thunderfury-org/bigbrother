use tracing::warn;

use crate::{
    application::{
        file_index::{SeenFile, is_permanent_index_source_error},
        import::{MetadataLookup, ShareUrl},
        import_media,
        share_crawler::ShareCrawler,
    },
    bootstrap::services::{
        FileIndexRuntimeService, KeywordService, NotifyService, ShareSourceService,
    },
    domain::import::inner::RawFile,
    error::AppResult,
    interface::telegram::file_index::{
        MediaSource, ProcessMediaSources, send_import_error, send_import_results,
    },
};

use super::ImportService;

#[derive(Clone)]
pub struct ProcessMediaSourcesHandler {
    pub file_index_service: FileIndexRuntimeService,
    pub share_crawler: ShareCrawler<ShareSourceService>,
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
        match import_media::import_with_raw_files(
            &mut handler.import_service,
            &mut handler.metadata_lookup,
            raw_files,
        )
        .await
        {
            Ok(imported) => {
                send_import_results(&handler.notify_service, reply_to, &imported).await;
            }
            Err(err) if is_permanent_index_source_error(&err) => {
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
            let parsed_url = url::Url::parse(url).map_err(|e| {
                crate::error::AppError::InvalidParameter(format!("invalid share url: {e}"))
            })?;
            let share_url = ShareUrl::from(&parsed_url).ok_or_else(|| {
                crate::error::AppError::InvalidParameter(format!("unsupported share url: {url}"))
            })?;
            handler
                .share_crawler
                .raw_files_from_share_url(&share_url)
                .await
        }
        MediaSource::Fslink(fslink) => handler.share_crawler.raw_files_from_fslink(fslink),
        MediaSource::TgDocument { file_id, file_name } => {
            return fetch_tg_document(handler, file_id, file_name, reply_to, error_prefix).await;
        }
    };

    match result {
        Ok(files) => Ok(Some(files)),
        Err(err) if is_permanent_index_source_error(&err) => {
            warn!(error = %err, "skipping permanent error");
            send_import_error(&handler.notify_service, reply_to, error_prefix, &err).await;
            Ok(None)
        }
        Err(err) => Err(err),
    }
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
        .await
        .map_err(|e| crate::error::AppError::Dependency(format!("failed to get document: {e}")))?;

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
    handler
        .bot
        .download_file(&file.path, &mut content)
        .await
        .map_err(|e| {
            crate::error::AppError::Dependency(format!("failed to download document: {e}"))
        })?;

    match handler.share_crawler.raw_files_from_json(content) {
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
