use crate::{error::AppResult, state::AppState};

mod category;
mod import;

pub use import::ImportSummary;
pub use import::json::is_fslink;
pub use import::share::ShareUrl;

pub async fn import_from_share_url(state: &AppState, url: &ShareUrl<'_>) -> AppResult<ImportSummary> {
    import::Importer::new(state.clone()).import_from_share_url(url).await
}

pub async fn import_from_fslink(state: &AppState, fslink: &str) -> AppResult<ImportSummary> {
    // Placeholder implementation
    import::Importer::new(state.clone()).import_from_fslink(fslink).await
}

pub async fn import_from_json(state: &AppState, json: Vec<u8>) -> AppResult<ImportSummary> {
    import::Importer::new(state.clone()).import_from_json(json).await
}
