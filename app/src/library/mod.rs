use crate::{error::AppResult, state::AppState};

mod import;
mod sync;

pub use import::ImportedMedia;
pub use import::json::is_fslink;
pub use import::share::ShareUrl;

pub async fn import_from_share_url(state: &AppState, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone()).import_from_share_url(url).await
}

pub async fn import_from_fslink(state: &AppState, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone()).import_from_fslink(fslink).await
}

pub async fn import_from_json(state: &AppState, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
    import::Importer::new(state.clone()).import_from_json(json).await
}

pub async fn sync_strm(state: &AppState) -> AppResult<()> {
    sync::Syncer::new(state.clone()).sync_strm().await
}
