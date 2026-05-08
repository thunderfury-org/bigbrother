mod providers;

use tracing::info;
use url::Url;

use super::{ImportedMedia, ShareImportUseCase};
use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::domain::import::{
    ShareUrl,
    inner::{MediaFile, RawFile},
    source::{
        parse_pan115_share_parts, parse_pan123_share_parts, parse_pan189_share_code,
        parse_quark_share_parts,
    },
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
        let (provider, media_files) = self.collect_media_files(url).await?;
        self.execute_import(provider, media_files).await
    }

    pub(crate) async fn raw_files_from_share_url(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<Vec<RawFile>> {
        match url {
            ShareUrl::Pan123(url) => {
                let (share_key, share_password) = parse_pan123_share_parts(url);
                self.raw_files_from_pan123_share(share_key.as_str(), share_password.as_str())
                    .await
            }
            ShareUrl::Pan189(url) => {
                let share_code = parse_pan189_share_code(url);
                if share_code.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share code from URL: {}",
                        url
                    )));
                }
                self.raw_files_from_pan189_share(&share_code).await
            }
            ShareUrl::Pan115(url) => {
                let (share_code, receive_code) = parse_pan115_share_parts(url);
                if share_code.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share code from URL: {}",
                        url
                    )));
                }
                self.raw_files_from_pan115_share(&share_code, &receive_code)
                    .await
            }
            ShareUrl::Quark(url) => {
                let (share_id, password) = parse_quark_share_parts(url);
                if share_id.is_empty() {
                    return Err(AppError::NotFound(format!(
                        "Can not extract share id from URL: {}",
                        url
                    )));
                }
                self.raw_files_from_quark_share(&share_id, &password).await
            }
        }
    }

    async fn collect_media_files(
        &mut self,
        url: &ShareUrl<'_>,
    ) -> AppResult<(&'static str, Vec<MediaFile>)> {
        match url {
            ShareUrl::Pan123(url) => self.collect_pan123_media_files(url).await,
            ShareUrl::Pan189(url) => self.collect_pan189_media_files(url).await,
            ShareUrl::Pan115(url) => self.collect_pan115_media_files(url).await,
            ShareUrl::Quark(url) => self.collect_quark_media_files(url).await,
        }
    }

    async fn collect_pan123_media_files(
        &mut self,
        url: &Url,
    ) -> AppResult<(&'static str, Vec<MediaFile>)> {
        let (share_key, share_password) = parse_pan123_share_parts(url);

        let media_files = self
            .list_files_from_pan123_share(share_key.as_str(), share_password.as_str())
            .await?;
        Ok(("pan123", media_files))
    }

    async fn collect_pan189_media_files(
        &mut self,
        url: &Url,
    ) -> AppResult<(&'static str, Vec<MediaFile>)> {
        let share_code = parse_pan189_share_code(url);
        if share_code.is_empty() {
            return Err(AppError::NotFound(format!(
                "Can not extract share code from URL: {}",
                url
            )));
        }

        let media_files = self.list_files_from_pan189_share(&share_code).await?;
        Ok(("pan189", media_files))
    }

    async fn collect_pan115_media_files(
        &mut self,
        url: &Url,
    ) -> AppResult<(&'static str, Vec<MediaFile>)> {
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
        Ok(("pan115", media_files))
    }

    async fn collect_quark_media_files(
        &mut self,
        url: &Url,
    ) -> AppResult<(&'static str, Vec<MediaFile>)> {
        let (share_id, password) = parse_quark_share_parts(url);
        let media_files = self
            .list_files_from_quark_share(&share_id, &password)
            .await?;
        Ok(("quark", media_files))
    }

    async fn execute_import(
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
