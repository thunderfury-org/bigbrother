use std::sync::Arc;

use teloxide::prelude::*;
use tracing::{error, info};

use crate::bootstrap::services::{ImportService, KeywordService, NotifyService, SyncService};

mod cmd;
pub(crate) mod delivery;
mod format;
mod msg;

struct BotServices {
    keyword: KeywordService,
    import: ImportService,
    notify: NotifyService,
    sync: SyncService,
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
    ) -> Self {
        Self {
            user_id,
            services: Arc::new(BotServices {
                keyword: keyword_service,
                import: import_service,
                notify: notify_service,
                sync: sync_service,
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

    let processor = msg::MsgProcessor {
        import_service: runtime.import_service(),
        notify_service: runtime.notify_service(),
        bot: &bot,
        msg: &msg,
        from_monitor: false,
    };
    processor.process().await
}
