use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use teloxide::net::Download;
use teloxide::{prelude::*, sugar::request::RequestReplyExt};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::{
    application::{delete_media::MediaDeleteCandidate, file_index::FileIndexSource},
    bootstrap::services::{
        DeleteMediaServiceRuntime, ImportService, KeywordService, NotifyService, SyncService,
    },
    infrastructure::event_bus::EventBus,
};

mod cmd;
pub(crate) mod delivery;
pub mod file_index;
mod msg;

struct BotServices {
    keyword: KeywordService,
    import: ImportService,
    notify: NotifyService,
    sync: SyncService,
    delete_media: DeleteMediaServiceRuntime,
    file_index_events: EventBus,
    file_index_ingest_dir: String,
    delete_media_cache: DeleteMediaCandidateCache,
}

#[derive(Clone)]
struct DeleteMediaCandidateCache {
    ttl: Duration,
    inner: Arc<RwLock<HashMap<i64, CachedDeleteMediaCandidate>>>,
}

#[derive(Clone)]
struct CachedDeleteMediaCandidate {
    candidate: MediaDeleteCandidate,
    expires_at: Instant,
}

#[derive(Clone)]
pub(crate) struct BotRuntime {
    pub user_id: UserId,
    services: Arc<BotServices>,
}

pub(crate) struct BotRuntimeArgs {
    pub user_id: UserId,
    pub keyword_service: KeywordService,
    pub import_service: ImportService,
    pub notify_service: NotifyService,
    pub sync_service: SyncService,
    pub delete_media_service: DeleteMediaServiceRuntime,
    pub file_index_events: EventBus,
    pub file_index_ingest_dir: String,
}

impl BotRuntime {
    pub(crate) fn new(args: BotRuntimeArgs) -> Self {
        Self {
            user_id: args.user_id,
            services: Arc::new(BotServices {
                keyword: args.keyword_service,
                import: args.import_service,
                notify: args.notify_service,
                sync: args.sync_service,
                delete_media: args.delete_media_service,
                file_index_events: args.file_index_events,
                file_index_ingest_dir: args.file_index_ingest_dir,
                delete_media_cache: DeleteMediaCandidateCache::new(Duration::from_secs(15 * 60)),
            }),
        }
    }

    fn keyword_service(&self) -> &KeywordService {
        &self.services.keyword
    }

    fn import_service(&self) -> &ImportService {
        &self.services.import
    }

    fn notify_service(&self) -> &NotifyService {
        &self.services.notify
    }

    fn sync_service(&self) -> &SyncService {
        &self.services.sync
    }

    fn delete_media_service(&self) -> &DeleteMediaServiceRuntime {
        &self.services.delete_media
    }

    fn file_index_event_bus(&self) -> &EventBus {
        &self.services.file_index_events
    }

    fn file_index_ingest_dir(&self) -> &str {
        &self.services.file_index_ingest_dir
    }

    async fn cache_delete_media_candidates(&self, candidates: &[MediaDeleteCandidate]) {
        self.services
            .delete_media_cache
            .insert_all(candidates)
            .await;
    }

    async fn get_delete_media_candidate(&self, dir_id: i64) -> Option<MediaDeleteCandidate> {
        self.services.delete_media_cache.get(dir_id).await
    }

    async fn remove_delete_media_candidate(&self, dir_id: i64) {
        self.services.delete_media_cache.remove(dir_id).await;
    }
}

impl DeleteMediaCandidateCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn insert_all(&self, candidates: &[MediaDeleteCandidate]) {
        let expires_at = Instant::now() + self.ttl;
        let mut guard = self.inner.write().await;
        prune_expired(&mut guard);
        for candidate in candidates {
            guard.insert(
                candidate.dir_id,
                CachedDeleteMediaCandidate {
                    candidate: candidate.clone(),
                    expires_at,
                },
            );
        }
    }

    async fn get(&self, dir_id: i64) -> Option<MediaDeleteCandidate> {
        let mut guard = self.inner.write().await;
        prune_expired(&mut guard);
        guard.get(&dir_id).map(|entry| entry.candidate.clone())
    }

    async fn remove(&self, dir_id: i64) {
        self.inner.write().await.remove(&dir_id);
    }
}

fn prune_expired(entries: &mut HashMap<i64, CachedDeleteMediaCandidate>) {
    let now = Instant::now();
    entries.retain(|_, entry| entry.expires_at > now);
}

pub(crate) async fn run(bot: teloxide::Bot, runtime: BotRuntime) {
    cmd::create_commands_in_background(&bot);

    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
        .branch(Update::filter_callback_query().endpoint(cmd::handle_callback_query))
        .branch(
            Update::filter_message()
                .filter_command::<cmd::Command>()
                .endpoint(cmd::handle_command),
        )
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![runtime])
        .build()
        .dispatch()
        .await;
}

async fn handle_channel_post(runtime: BotRuntime, bot: Bot, msg: Message) -> ResponseResult<()> {
    publish_file_index_event(&runtime, &bot, &msg).await;

    let keywords = match runtime.keyword_service().list_values().await {
        Ok(keywords) => keywords,
        Err(e) => {
            error!("Failed to query keywords from database: {e}");
            return Ok(());
        }
    };

    if keywords.is_empty() {
        return Ok(());
    }
    let text = msg.text().or(msg.caption()).unwrap_or_default();
    for keyword in &keywords {
        if text.contains(keyword) {
            let processor = msg::MsgProcessor {
                import_service: runtime.import_service(),
                notify_service: runtime.notify_service(),
                bot: &bot,
                msg: &msg,
                from_monitor: true,
            };
            return processor.process().await;
        }
    }

    if let Some(doc) = msg.document()
        && let Some(text) = doc.file_name.as_ref()
        && text.ends_with(".json")
    {
        for keyword in &keywords {
            if text.contains(keyword) {
                let processor = msg::MsgProcessor {
                    import_service: runtime.import_service(),
                    notify_service: runtime.notify_service(),
                    bot: &bot,
                    msg: &msg,
                    from_monitor: true,
                };
                return processor.process().await;
            }
        }
    }

    Ok(())
}

async fn handle_message(runtime: BotRuntime, bot: Bot, msg: Message) -> ResponseResult<()> {
    info!("Received message from {:?}", msg.chat);

    if msg.from.as_ref().is_none_or(|u| u.id != runtime.user_id) {
        info!("Ignoring message from unauthorized user: {:?}", msg.from);
        return Ok(());
    }

    publish_file_index_event(&runtime, &bot, &msg).await;

    if cmd::is_delete_media_usage_request(&msg) {
        bot.send_message(msg.chat.id, cmd::delete_media_usage())
            .reply_to(msg.id)
            .await?;
        return Ok(());
    }

    let processor = msg::MsgProcessor {
        import_service: runtime.import_service(),
        notify_service: runtime.notify_service(),
        bot: &bot,
        msg: &msg,
        from_monitor: false,
    };
    processor.process().await
}

async fn publish_file_index_event(runtime: &BotRuntime, bot: &Bot, msg: &Message) {
    let mut sources = file_index::extract_index_sources(msg);
    if let Some(source) = download_document_index_source(runtime, bot, msg).await {
        sources.push(source);
    }
    if sources.is_empty() {
        return;
    }

    let event = file_index::IndexFilesFromSource {
        sources,
        description: file_index::message_description(msg),
    };
    if let Err(err) = runtime.file_index_event_bus().publish(&event).await {
        error!("Failed to publish file index event: {}", err);
    }
}

async fn download_document_index_source(
    runtime: &BotRuntime,
    bot: &Bot,
    msg: &Message,
) -> Option<FileIndexSource> {
    let doc = msg.document()?;
    let file_name = doc.file_name.as_deref()?;
    if !is_index_document(file_name) {
        return None;
    }

    let file = match bot.get_file(doc.file.id.to_owned()).await {
        Ok(file) => file,
        Err(err) => {
            error!("Failed to get telegram document for file indexing: {}", err);
            return None;
        }
    };
    if file.meta.size > 10 * 1024 * 1024 {
        error!(
            "Telegram document is too large for file indexing: {}",
            file_name
        );
        return None;
    }

    let ingest_dir = runtime.file_index_ingest_dir();
    if let Err(err) = tokio::fs::create_dir_all(ingest_dir).await {
        error!(
            "Failed to create file index ingest dir '{}': {}",
            ingest_dir, err
        );
        return None;
    }

    let local_path = format!(
        "{}/{}-{}-{}",
        ingest_dir,
        msg.id.0,
        chrono::Utc::now().timestamp_millis(),
        sanitize_file_name(file_name),
    );
    let mut content = Vec::with_capacity(file.meta.size.try_into().unwrap_or_default());
    if let Err(err) = bot.download_file(&file.path, &mut content).await {
        error!(
            "Failed to download telegram document for file indexing: {}",
            err
        );
        return None;
    }
    if let Err(err) = tokio::fs::write(&local_path, content).await {
        error!(
            "Failed to write telegram document index source '{}': {}",
            local_path, err
        );
        return None;
    }

    Some(FileIndexSource::LocalJsonFile(local_path))
}

fn is_index_document(file_name: &str) -> bool {
    let name = file_name.to_lowercase();
    name.ends_with(".json") || name.ends_with(".cas")
}

fn sanitize_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}
