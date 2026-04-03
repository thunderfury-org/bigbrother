use teloxide::prelude::*;
use tracing::{error, info};

use crate::{
    application::{
        import_media::ImportMediaService, manage_keywords::ManageKeywordsService,
        notify::PublishTelegramMessageService, sync_strm::SyncStrmService,
    },
    infrastructure::{
        client::library_remote::Pan123LibraryRemote, event::publisher::EventBusPublisher,
        fs::tokio_file_store::TokioFileStore, import::gateway::AppStateImportGateway,
        repo::keyword::SeaOrmKeywordRepository,
    },
    state::AppState,
};

mod cmd;
mod format;
pub mod handler;
mod msg;

#[derive(Clone)]
pub(crate) struct BotRuntime {
    pub user_id: UserId,
    pub keyword_repo: SeaOrmKeywordRepository,
    pub import_gateway: AppStateImportGateway,
    pub notify_publisher: EventBusPublisher,
    pub sync_remote: Pan123LibraryRemote,
    pub sync_file_store: TokioFileStore,
    pub sync_config: crate::application::sync_strm::SyncStrmConfig,
}

impl BotRuntime {
    pub(crate) fn from_state(state: AppState) -> Self {
        Self {
            user_id: UserId(
                state
                    .config()
                    .get_telegram_config()
                    .user_id
                    .try_into()
                    .unwrap(),
            ),
            keyword_repo: SeaOrmKeywordRepository::new(state.db().clone()),
            import_gateway: AppStateImportGateway::new(state.clone()),
            notify_publisher: EventBusPublisher::new(state.bus().clone()),
            sync_remote: Pan123LibraryRemote::new(state.client().pan123.clone()),
            sync_file_store: TokioFileStore,
            sync_config: crate::application::sync_strm::SyncStrmConfig {
                remote_path: state.config().get_library_config().remote_path.clone(),
                local_path: state.config().get_library_config().local_path.clone(),
                strm_download_url: state
                    .config()
                    .get_media_server_config()
                    .get_strm_download_url(),
            },
        }
    }

    fn keyword_service(&self) -> ManageKeywordsService<SeaOrmKeywordRepository> {
        ManageKeywordsService::new(self.keyword_repo.clone())
    }

    fn import_service(&self) -> ImportMediaService<AppStateImportGateway> {
        ImportMediaService::new(self.import_gateway.clone())
    }

    fn notify_service(&self) -> PublishTelegramMessageService<EventBusPublisher> {
        PublishTelegramMessageService::new(self.notify_publisher.clone())
    }

    fn sync_service(&self) -> SyncStrmService<Pan123LibraryRemote, TokioFileStore> {
        SyncStrmService::new(
            self.sync_remote.clone(),
            self.sync_file_store,
            self.sync_config.clone(),
        )
    }
}

pub async fn run(bot: teloxide::Bot, runtime: BotRuntime) {
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
        // Ignore messages not from the specified user
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
