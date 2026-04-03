use crate::{
    application::import_media::ImportMediaGateway,
    error::AppResult,
    library::{ImportedMedia, ShareUrl, import},
    state::AppState,
};

#[derive(Clone)]
pub struct AppStateImportGateway {
    state: AppState,
}

impl AppStateImportGateway {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl ImportMediaGateway for AppStateImportGateway {
    async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::new(self.state.clone())
            .import_from_share_url(url)
            .await
    }

    async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::new(self.state.clone())
            .import_from_fslink(fslink)
            .await
    }

    async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::new(self.state.clone())
            .import_from_json(json)
            .await
    }
}
