use std::collections::HashMap;

use crate::application::import_ports::{
    ImportLocalStore, LibraryGateway, MetadataCatalog, ShareSource,
};
use crate::domain::import::{
    inner::{MediaFile, RawFile},
    policy::group_video_and_subtitle_files,
};
use crate::domain::media::Metadata;

use super::Importer;

#[derive(Default)]
pub(super) struct MetadataLookup {
    cache: HashMap<String, Box<Metadata>>,
}

impl MetadataLookup {
    pub(super) fn parse_media_metadata(&mut self, name: &str, parent_path: &str) -> Box<Metadata> {
        let mut meta = Metadata::parse(name);
        if parent_path.is_empty() {
            return meta;
        }

        let path_meta = self.parse_metadata_from_path(parent_path, meta.episode_number.is_some());
        meta.merge_metadata(&path_meta);
        meta
    }

    fn parse_metadata_from_path(&mut self, parent_path: &str, is_tv: bool) -> Box<Metadata> {
        if let Some(meta) = self.cache.get(parent_path) {
            return meta.clone();
        }

        let parts = parent_path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Box::new(Metadata::default());
        }

        let mut meta = Metadata::parse(parts.last().unwrap());
        if !is_tv || parts.len() < 2 {
            self.cache.insert(parent_path.to_string(), meta.clone());
            return meta;
        }

        let path_meta = Metadata::parse(parts[parts.len() - 2]);
        meta.merge_metadata(&path_meta);

        self.cache.insert(parent_path.to_string(), meta.clone());
        meta
    }

    fn build_media_files(&mut self, raw_files: Vec<RawFile>) -> Vec<MediaFile> {
        let mut parsed_files = Vec::new();

        for raw_file in raw_files {
            let metadata = self.parse_media_metadata(&raw_file.name, &raw_file.path);
            if metadata.unknown_type() {
                continue;
            }

            parsed_files.push((metadata, raw_file));
        }

        group_video_and_subtitle_files(parsed_files)
    }
}

impl<L, S, M, F> Importer<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub(super) fn build_media_files(&mut self, raw_files: Vec<RawFile>) -> Vec<MediaFile> {
        self.metadata_lookup.build_media_files(raw_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::import::inner::Etag;

    fn raw_file(name: &str, path: &str) -> RawFile {
        RawFile {
            id: None,
            name: name.to_string(),
            etag: Etag::Md5("etag".to_string()),
            size: 1,
            path: path.to_string(),
        }
    }

    #[test]
    fn build_media_files_filters_unknown_files() {
        let mut lookup = MetadataLookup::default();

        let media_files = lookup.build_media_files(vec![
            raw_file("Movie.2020.1080p.mkv", ""),
            raw_file("notes.txt", ""),
        ]);

        assert_eq!(media_files.len(), 1);
        assert_eq!(media_files[0].video.name, "Movie.2020.1080p.mkv");
    }

    #[test]
    fn build_media_files_groups_subtitles_after_metadata_lookup() {
        let mut lookup = MetadataLookup::default();

        let media_files = lookup.build_media_files(vec![
            raw_file("Show.S01E01.mkv", "/Show"),
            raw_file("Show.S01E01.zh.srt", "/Show"),
        ]);

        assert_eq!(media_files.len(), 1);
        assert_eq!(media_files[0].subtitles.len(), 1);
        assert_eq!(media_files[0].subtitles[0].name, "Show.S01E01.zh.srt");
    }
}
