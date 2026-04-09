use crate::{
    application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource},
    error::AppResult,
    library::{
        ImportedMedia, ShareUrl,
        import::{ImportPathConfig, Importer},
    },
};

#[derive(Clone)]
pub struct ImportMediaService<L, S, M> {
    library_gateway: L,
    share_source: S,
    metadata_catalog: M,
    paths: ImportPathConfig,
}

impl<L, S, M> ImportMediaService<L, S, M> {
    pub fn new(
        library_gateway: L,
        share_source: S,
        metadata_catalog: M,
        paths: ImportPathConfig,
    ) -> Self {
        Self {
            library_gateway,
            share_source,
            metadata_catalog,
            paths,
        }
    }
}

impl<L, S, M> ImportMediaService<L, S, M>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
{
    fn importer(&self) -> Importer<L, S, M> {
        Importer::new(
            self.library_gateway.clone(),
            self.share_source.clone(),
            self.metadata_catalog.clone(),
            self.paths.clone(),
        )
    }

    pub async fn import_from_share_url(&self, url: &ShareUrl<'_>) -> AppResult<Vec<ImportedMedia>> {
        self.importer().import_from_share_url(url).await
    }

    pub async fn import_from_fslink(&self, fslink: &str) -> AppResult<Vec<ImportedMedia>> {
        self.importer().import_from_fslink(fslink).await
    }

    pub async fn import_from_json(&self, json: Vec<u8>) -> AppResult<Vec<ImportedMedia>> {
        self.importer().import_from_json(json).await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    use reqwest::Url;

    use super::*;
    use crate::application::import_ports::{LibraryGateway, MetadataCatalog, ShareSource};
    use crate::library::import::{
        LibraryFile, MovieDetail, Pan115FileEntry, Pan189File, Pan189Folder, Pan189ShareInfo,
        SearchMovieResult, SearchTvResult, TvDetail,
    };

    #[derive(Clone, Default)]
    struct FakeLibraryGateway {
        state: Arc<Mutex<FakeLibraryState>>,
    }

    #[derive(Default)]
    struct FakeLibraryState {
        mkdir_paths: Vec<String>,
        fast_uploads: Vec<(i64, String, String, u64)>,
    }

    #[derive(Clone, Default)]
    struct FakeShareSource {
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[derive(Clone, Default)]
    struct FakeMetadataCatalog;

    impl LibraryGateway for FakeLibraryGateway {
        async fn list_library_files(&self, _dir_id: i64) -> AppResult<Vec<LibraryFile>> {
            Ok(Vec::new())
        }
        async fn get_library_dir_id_by_path(&self, _path: &str) -> AppResult<Option<i64>> {
            Ok(None)
        }
        async fn mkdir_library_path(&self, path: &str) -> AppResult<i64> {
            self.state
                .lock()
                .unwrap()
                .mkdir_paths
                .push(path.to_string());
            Ok(1)
        }
        async fn list_library_dir_ids(
            &self,
            _dir_id: i64,
        ) -> AppResult<std::collections::HashMap<String, i64>> {
            Ok(Default::default())
        }
        async fn mkdir_library_dir(
            &self,
            _parent_dir_id: i64,
            _folder_name: &str,
        ) -> AppResult<i64> {
            Ok(1)
        }
        async fn trash_library_files(&self, _file_ids: &[i64]) -> AppResult<()> {
            Ok(())
        }
        async fn fast_upload_md5(
            &self,
            parent_dir_id: i64,
            file_name: &str,
            etag: &str,
            size: u64,
        ) -> AppResult<Option<i64>> {
            self.state.lock().unwrap().fast_uploads.push((
                parent_dir_id,
                file_name.to_string(),
                etag.to_string(),
                size,
            ));
            Ok(Some(42))
        }
        async fn fast_upload_sha1(
            &self,
            _parent_dir_id: i64,
            _file_name: &str,
            _sha1: &str,
            _size: u64,
        ) -> AppResult<Option<i64>> {
            Ok(None)
        }
        async fn download_library_file(&self, _file_id: i64, _local_path: &str) -> AppResult<()> {
            Ok(())
        }
    }

    impl ShareSource for FakeShareSource {
        async fn list_pan123_share_files(
            &self,
            _share_key: &str,
            _share_password: &str,
            _parent_id: i64,
        ) -> AppResult<Vec<LibraryFile>> {
            self.calls.lock().unwrap().push("share".to_string());
            Ok(Vec::new())
        }
        async fn get_pan189_share_info(&self, _share_code: &str) -> AppResult<Pan189ShareInfo> {
            self.calls.lock().unwrap().push("share".to_string());
            Ok(Default::default())
        }
        async fn list_pan189_share_files(
            &self,
            _share_id: i64,
            _share_mode: i32,
            _parent_id: &str,
        ) -> AppResult<(Vec<Pan189Folder>, Vec<Pan189File>)> {
            self.calls.lock().unwrap().push("share".to_string());
            Ok((Vec::new(), Vec::new()))
        }
        async fn list_pan115_share_files(
            &self,
            _share_code: &str,
            _receive_code: &str,
            _cid: &str,
        ) -> AppResult<Vec<Pan115FileEntry>> {
            self.calls.lock().unwrap().push("share".to_string());
            Ok(Vec::new())
        }
    }

    impl MetadataCatalog for FakeMetadataCatalog {
        async fn search_movie(&self, title: &str, year: &str) -> AppResult<Vec<SearchMovieResult>> {
            if title == "Inception" && year == "2010" {
                Ok(vec![SearchMovieResult {
                    id: 27205,
                    title: "Inception".into(),
                    original_title: "Inception".into(),
                }])
            } else {
                Ok(Vec::new())
            }
        }
        async fn get_movie_detail(&self, id: u32) -> AppResult<Option<MovieDetail>> {
            Ok((id == 27205).then_some(MovieDetail {
                id,
                title: "Inception".into(),
                adult: false,
                genres: Vec::new(),
                original_language: "en".into(),
                original_title: "Inception".into(),
                origin_country: vec!["US".into()],
                release_date: "2010-07-16".into(),
            }))
        }
        async fn search_tv(&self, _title: &str, _year: &str) -> AppResult<Vec<SearchTvResult>> {
            Ok(Vec::new())
        }
        async fn get_tv_detail(&self, _id: u32) -> AppResult<Option<TvDetail>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn service_delegates_to_gateway() {
        let share_source = FakeShareSource::default();
        let service = ImportMediaService::new(
            FakeLibraryGateway::default(),
            share_source.clone(),
            FakeMetadataCatalog,
            ImportPathConfig::new("/remote".into(), "/local".into(), "http://localhost".into()),
        );
        let url = Url::parse("https://www.123684.com/s/test").unwrap();
        let share = ShareUrl::from(&url).unwrap();

        service.import_from_share_url(&share).await.unwrap();

        assert_eq!(share_source.calls.lock().unwrap().as_slice(), ["share"]);
    }

    fn unique_temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("bigbrother-import-test-{suffix}"))
    }

    #[tokio::test]
    async fn import_from_json_writes_strm_and_records_upload() {
        let local_dir = unique_temp_dir();
        let gateway = FakeLibraryGateway::default();
        let service = ImportMediaService::new(
            gateway.clone(),
            FakeShareSource::default(),
            FakeMetadataCatalog,
            ImportPathConfig::new(
                "/remote".into(),
                local_dir.to_string_lossy().into_owned(),
                "http://localhost/d".into(),
            ),
        );

        let json = serde_json::to_vec(&serde_json::json!({
            "files": [{
                "path": "Inception.2010.1080p.mkv",
                "etag": "0123456789abcdef0123456789abcdef",
                "size": 1234u64
            }]
        }))
        .unwrap();

        let imported = service.import_from_json(json).await.unwrap();

        assert!(matches!(
            imported.as_slice(),
            [ImportedMedia::Movie {
                title,
                year,
                size,
                has_failed,
                ..
            }] if title == "Inception" && year == "2010" && *size == 1234 && !has_failed
        ));

        let state = gateway.state.lock().unwrap();
        assert_eq!(
            state.mkdir_paths,
            vec!["/remote/电影/欧美/Inception (2010) {tmdb-27205}".to_string()]
        );
        assert_eq!(
            state.fast_uploads,
            vec![(
                1,
                "Inception.2010.1080p.mkv".to_string(),
                "0123456789abcdef0123456789abcdef".to_string(),
                1234
            )]
        );
        drop(state);

        let strm_path =
            local_dir.join("电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.strm");
        let strm_content = fs::read_to_string(&strm_path).unwrap();
        assert_eq!(
            strm_content,
            "http://localhost/d/remote/电影/欧美/Inception (2010) {tmdb-27205}/Inception.2010.1080p.mkv?file_id=42"
        );

        let _ = fs::remove_dir_all(local_dir);
    }
}
