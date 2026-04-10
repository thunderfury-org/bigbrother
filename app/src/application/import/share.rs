mod providers;

use reqwest::Url;
use tracing::info;

use super::{ImportedMedia, ShareImportUseCase};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::domain::import::{
    ShareUrl,
    inner::MediaFile,
    source::{parse_pan115_share_parts, parse_pan123_share_parts, parse_pan189_share_code},
};
use crate::error::{AppError, AppResult};

impl<L, S, M, F> ShareImportUseCase<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub async fn import_from_share_url(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from share URL: {}", url.get_url());
        match url {
            ShareUrl::Pan123(url) => self.import_pan123_share(url).await,
            ShareUrl::Pan189(url) => self.import_pan189_share(url).await,
            ShareUrl::Pan115(url) => self.import_pan115_share(url).await,
        }
    }

    async fn import_pan123_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_key, share_password) = parse_pan123_share_parts(url);

        let media_files = self
            .list_files_from_pan123_share(share_key.as_str(), share_password.as_str())
            .await?;
        self.finish_share_import("pan123", media_files).await
    }

    async fn import_pan189_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let share_code = parse_pan189_share_code(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self.list_files_from_pan189_share(&share_code).await?;
        self.finish_share_import("pan189", media_files).await
    }

    async fn import_pan115_share(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let (share_code, receive_code) = parse_pan115_share_parts(url);

        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self
            .list_files_from_pan115_share(&share_code, &receive_code)
            .await?;
        self.finish_share_import("pan115", media_files).await
    }

    async fn finish_share_import(
        &mut self,
        provider: &str,
        media_files: Vec<MediaFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        info!(
            "found {} media files from {} share",
            media_files.len(),
            provider
        );
        self.transfer_mut().transfer_media_files(&media_files).await
    }
}
