use crate::{
    application::sync_strm::{SyncStrmConfig, SyncStrmService},
    error::AppResult,
    infrastructure::{
        client::library_remote::Pan123LibraryRemote, fs::tokio_file_store::TokioFileStore,
    },
    state::AppState,
};

pub(crate) mod import;

pub use import::ImportedMedia;
pub use import::json::is_fslink;
pub use import::share::ShareUrl;

pub async fn sync_strm(state: &AppState) -> AppResult<()> {
    SyncStrmService::new(
        Pan123LibraryRemote::new(state.client().pan123.clone()),
        TokioFileStore,
        SyncStrmConfig {
            remote_path: state.config().get_library_config().remote_path.clone(),
            local_path: state.config().get_library_config().local_path.clone(),
            strm_download_url: state
                .config()
                .get_media_server_config()
                .get_strm_download_url(),
        },
    )
    .execute()
    .await
}
