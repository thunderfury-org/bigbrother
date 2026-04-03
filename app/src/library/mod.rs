use crate::{
    application::sync_strm::{SyncStrmConfig, SyncStrmService},
    error::AppResult,
    infrastructure::{
        client::library_remote::Pan123LibraryRemote, fs::tokio_file_store::TokioFileStore,
    },
    state::AppState,
};

mod import;

pub use import::ImportedMedia;
pub use import::json::is_fslink;
pub use import::share::ShareUrl;

pub async fn import_from_share_url(
    state: &AppState,
    url: &ShareUrl<'_>,
) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone())
        .import_from_share_url(url)
        .await
}

pub async fn import_from_fslink(state: &AppState, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone())
        .import_from_fslink(fslink)
        .await
}

pub async fn import_from_json(state: &AppState, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone())
        .import_from_json(json)
        .await
}

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
