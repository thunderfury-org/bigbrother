use teloxide::prelude::*;
use tracing::{error, info};

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

pub(crate) async fn run(bot: teloxide::Bot, runtime: BotRuntime) {
    let handler = dptree::entry()
        .branch(Update::filter_channel_post().endpoint(handle_channel_post))
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
}
