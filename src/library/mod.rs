use reqwest::Url;

use crate::{error::AppResult, state::AppState};

mod category;
mod import;

#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub success: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub total_size: u64,
    pub cost: std::time::Duration,
    pub unknown_files: Vec<String>,
}

pub async fn import_from_share_url(state: &AppState, url: &Url) -> AppResult<ImportSummary> {
    import::Importer::new(state.clone()).import_from_share_url(url).await
}

pub async fn import_from_fslink(state: &AppState, fslink: &str) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}

pub async fn import_from_remote_dir(state: &AppState, dir: &str) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}
