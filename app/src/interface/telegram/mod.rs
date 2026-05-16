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
    infrastructure::event_bus::EventBus,
    infrastructure::services::{
        DeleteMediaServiceRuntime, KeywordService, NotifyService, SyncService,
    },
};

mod cmd;
pub(crate) mod delivery;
pub mod file_index;
pub(crate) mod handler;

const NO_VALID_MEDIA_SOURCE_MESSAGE: &str =
    "未发现有效分享链接，仅支持 Pan123、天翼、115、夸克分享链接，或 fslink、.json/.cas 文件";

struct BotServices {
    keyword: KeywordService,
    notify: NotifyService,
    sync: SyncService,
    delete_media: DeleteMediaServiceRuntime,
    event_bus: EventBus,
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
    pub notify_service: NotifyService,
    pub sync_service: SyncService,
    pub delete_media_service: DeleteMediaServiceRuntime,
    pub event_bus: EventBus,
}

impl BotRuntime {
    pub(crate) fn new(args: BotRuntimeArgs) -> Self {
        Self {
            user_id: args.user_id,
            services: Arc::new(BotServices {
                keyword: args.keyword_service,
                notify: args.notify_service,
                sync: args.sync_service,
                delete_media: args.delete_media_service,
                event_bus: args.event_bus,
                delete_media_cache: DeleteMediaCandidateCache::new(Duration::from_secs(15 * 60)),
            }),
        }
    }

    fn keyword_service(&self) -> &KeywordService {
        &self.services.keyword
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

    fn event_bus(&self) -> &EventBus {
        &self.services.event_bus
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

async fn handle_channel_post(runtime: BotRuntime, msg: Message) -> ResponseResult<()> {
    let sources = file_index::extract_media_sources(&msg);
    if sources.is_empty() {
        info!("Skipping channel message without media sources");
        return Ok(());
    }

    let description = file_index::message_description(&msg);

    for source in sources {
        let event = file_index::ProcessMediaSources {
            source,
            description: description.clone(),
            channel_post: true,
            reply_to_message_id: None,
        };
        if let Err(err) = runtime.event_bus().publish(&event).await {
            error!("Failed to publish ProcessMediaSources event: {err}");
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

    let sources = file_index::extract_media_sources(&msg);
    if sources.is_empty() {
        if let Err(err) = runtime
            .notify_service()
            .send_message(
                NO_VALID_MEDIA_SOURCE_MESSAGE,
                msg.from.as_ref().map(|_| msg.id.0),
            )
            .await
        {
            error!("Failed to send no-valid-media-source message: {err}");
        }
        return Ok(());
    }

    let description = file_index::message_description(&msg);
    let reply_to = msg.from.as_ref().map(|_| msg.id.0);
    let confirm = import_start_message(&sources);
    let total_sources = sources.len();
    let mut published_sources = 0usize;

    for source in sources {
        let event = file_index::ProcessMediaSources {
            source,
            description: description.clone(),
            channel_post: false,
            reply_to_message_id: reply_to,
        };
        match runtime.event_bus().publish(&event).await {
            Ok(()) => {
                published_sources += 1;
                if published_sources == total_sources
                    && let Err(err) = runtime
                        .notify_service()
                        .send_message(&confirm, reply_to)
                        .await
                {
                    error!("Failed to send instant confirmation: {err}");
                }
            }
            Err(err) => {
                error!("Failed to publish ProcessMediaSources event: {err}");
            }
        }
    }

    Ok(())
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
    use super::{NO_VALID_MEDIA_SOURCE_MESSAGE, import_start_message};
    use crate::interface::telegram::file_index::MediaSource;

    #[test]
    fn import_start_message_is_specific_for_single_share_url() {
        let message = import_start_message(&[MediaSource::ShareUrl(
            "https://pan.quark.cn/s/share-id?pwd=abc".to_string(),
        )]);

        assert_eq!(
            message,
            "开始处理分享: https://pan.quark.cn/s/share-id?pwd=abc"
        );
    }

    #[test]
    fn import_start_message_summarizes_multiple_media_sources() {
        let message = import_start_message(&[
            MediaSource::ShareUrl("https://pan.quark.cn/s/share-id?pwd=abc".to_string()),
            MediaSource::Fslink(
                "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv".into(),
            ),
        ]);

        assert_eq!(message, "发现 2 个有效来源，开始处理");
    }

    #[test]
    fn no_valid_media_source_message_mentions_supported_inputs() {
        assert_eq!(
            NO_VALID_MEDIA_SOURCE_MESSAGE,
            "未发现有效分享链接，仅支持 Pan123、天翼、115、夸克分享链接，或 fslink、.json/.cas 文件"
        );
    }
}
