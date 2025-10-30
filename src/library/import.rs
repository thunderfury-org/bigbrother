use reqwest::Url;

use crate::{error::AppResult, state::AppState};

#[derive(Debug)]
pub struct FSLinkFile {
    pub path: String,
    pub etag: String,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct ImportSummary {
    pub catelog: String,
    pub title: String,
    pub year: String,
    pub success: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total: u32,
    pub total_size: u64,
    pub cost: std::time::Duration,
}

pub async fn import_from_share_url(state: &AppState, url: &Url) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}

pub async fn import_from_fslink(state: &AppState, files: Vec<FSLinkFile>) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}

pub async fn import_from_remote_dir(state: &AppState, dir: &str) -> AppResult<ImportSummary> {
    // Placeholder implementation
    Ok(ImportSummary::default())
}
