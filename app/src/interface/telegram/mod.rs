use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use teloxide::{prelude::*, sugar::request::RequestReplyExt};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::{
    application::delete_media::MediaDeleteCandidate,
    bootstrap::services::{
        DeleteMediaServiceRuntime, ImportService, KeywordService, NotifyService, SyncService,
    },
};

mod cmd;
pub(crate) mod delivery;
mod msg;

struct BotServices {
    keyword: KeywordService,
    import: ImportService,
    notify: NotifyService,
    sync: SyncService,
    delete_media: DeleteMediaServiceRuntime,
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

impl BotRuntime {
    pub(crate) fn new(
        user_id: UserId,
        keyword_service: KeywordService,
        import_service: ImportService,
        notify_service: NotifyService,
        sync_service: SyncService,
        delete_media_service: DeleteMediaServiceRuntime,
    ) -> Self {
        Self {
            user_id,
            services: Arc::new(BotServices {
                keyword: keyword_service,
                import: import_service,
                notify: notify_service,
                sync: sync_service,
                delete_media: delete_media_service,
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
