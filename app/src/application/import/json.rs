use std::path::Path;

use tracing::info;

use super::{ImportedMedia, JsonImportUseCase};
use crate::application::import_ports::{ImportLocalStore, LibraryGateway, MetadataCatalog};
use crate::domain::import::{
    inner::{MediaFile, RawFile},
    source::{ResourceJson, parse_files_from_fslink, parse_files_from_json},
};
use crate::error::AppResult;

impl<L, M, F> JsonImportUseCase<L, M, F>
where
    L: LibraryGateway,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub async fn import_from_fslink(&mut self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from fslink");
        let resource = self.parse_fslink_resource(fslink)?;
        self.import_from_resource_json(&resource).await
    }

    pub async fn import_from_json(&mut self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        info!("Importing from JSON");
        let resource: ResourceJson = parse_files_from_json(json)?;
        self.import_from_resource_json(&resource).await
    }

    fn parse_fslink_resource(&self, fslink: &str) -> AppResult<ResourceJson> {
        let mut resource = ResourceJson::default();

        let mut fslink = fslink.find("$").map(|i| &fslink[i + 1..]).unwrap_or(fslink);
        if let Some(i) = fslink.find("%") {
            resource.common_path = fslink[..i].to_owned();
            fslink = &fslink[i + 1..];
        }
        resource.files = parse_files_from_fslink(fslink)?;
        Ok(resource)
    }

    async fn import_from_resource_json(
        &mut self,
        resource: &ResourceJson,
    ) -> AppResult<Vec<ImportedMedia>> {
        let media_files = self.normalize_resource_files(resource);
        self.execute_import(media_files).await
    }

    fn normalize_resource_files(&mut self, resource: &ResourceJson) -> Vec<MediaFile> {
        let mut raw_files = Vec::new();

        for file in &resource.files {
            let _path = format!("{}/{}", &resource.common_path, &file.path);
            let path = Path::new(_path.as_str());
            let parent_path = path
                .parent()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();
            let name = path
                .file_name()
                .map(|p| p.to_str().unwrap_or_default())
                .unwrap_or_default();

            raw_files.push(RawFile {
                id: None,
                name: name.to_owned(),
                etag: file.etag.as_str().into(),
                size: file.size,
                path: parent_path.to_owned(),
            });
        }

        self.metadata_lookup_mut().build_media_files(raw_files)
    }

    async fn execute_import(
        &mut self,
        media_files: Vec<MediaFile>,
    ) -> AppResult<Vec<ImportedMedia>> {
        self.transfer_mut().transfer_media_files(&media_files).await
    }
}
