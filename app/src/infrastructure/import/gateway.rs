use crate::{
    application::import_media::ImportMediaGateway,
    error::AppResult,
    library::{
        ImportedMedia, ShareUrl,
        import::{self, ImportContext},
    },
};

#[derive(Clone)]
pub struct ImportGateway {
    ctx: ImportContext,
}

impl ImportGateway {
    pub fn new(ctx: ImportContext) -> Self {
        Self { ctx }
    }
}

impl ImportMediaGateway for ImportGateway {
    async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::from_context(self.ctx.clone())
            .import_from_share_url(url)
            .await
    }

    async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::from_context(self.ctx.clone())
            .import_from_fslink(fslink)
            .await
    }

    async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        import::Importer::from_context(self.ctx.clone())
            .import_from_json(json)
            .await
    }
}
