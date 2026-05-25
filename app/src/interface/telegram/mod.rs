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
    application::notify::{Message as OutboundMessage, MessageSender},
    infrastructure::event_bus::EventBus,
    infrastructure::services::{
        DeleteMediaServiceRuntime, KeywordService, NotifyService, SyncService,
    },
};

mod cmd;
pub(crate) mod delivery;
pub mod export;
pub mod file_index;
pub(crate) mod handler;

const NO_VALID_MEDIA_SOURCE_MESSAGE: &str =
    "未发现有效分享链接，仅支持 Pan123、天翼、115、夸克分享链接，或 fslink、.json/.cas 文件";

#[derive(Debug, PartialEq, Eq)]
enum SourceHandling {
    Ignore,
    NotifyNoValidMediaSource,
    Process { confirm: String },
}

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
    fn private_message_without_media_sources_replies_once_with_supported_input_hint() {
        assert_eq!(
            decide_source_handling(false, &[]),
            SourceHandling::NotifyNoValidMediaSource
        );
        assert_eq!(
            NO_VALID_MEDIA_SOURCE_MESSAGE,
            "未发现有效分享链接，仅支持 Pan123、天翼、115、夸克分享链接，或 fslink、.json/.cas 文件"
        );
    }

    #[test]
    fn channel_post_without_media_sources_is_silently_ignored() {
        assert_eq!(decide_source_handling(true, &[]), SourceHandling::Ignore);
    }

    #[test]
    fn mixed_supported_and_unsupported_inputs_only_process_supported_sources() {
        let msg: Message = serde_json::from_value(json!({
            "message_id": 1,
            "date": 1_700_000_000,
            "chat": {
                "id": 42,
                "type": "private"
            },
            "text": "https://pan.quark.cn/s/share-id?pwd=abc\nhttps://www.themoviedb.org/tv/314784"
        }))
        .unwrap();

        let sources = extract_media_sources(&msg);
        let handling = decide_source_handling(false, &sources);

        assert_eq!(
            sources,
            vec![MediaSource::ShareUrl(
                "https://pan.quark.cn/s/share-id?pwd=abc".to_string()
            )]
        );
        assert_eq!(
            handling,
            SourceHandling::Process {
                confirm: "开始处理分享: https://pan.quark.cn/s/share-id?pwd=abc".to_string()
            }
        );
    }
}
