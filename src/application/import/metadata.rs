mod path;

use std::collections::HashMap;

use crate::domain::import::{inner::MediaFile, policy::group_video_and_subtitle_files};
use crate::domain::media::Metadata;
use crate::domain::share::RawFile;

#[derive(Clone, Default)]
pub(crate) struct MetadataLookup {
    cache: HashMap<String, Box<Metadata>>,
}

impl MetadataLookup {
    fn parse_media_metadata(&mut self, name: &str, parent_path: &str) -> Box<Metadata> {
        let mut meta = Metadata::parse(name);
        if parent_path.is_empty() {
            return meta;
        }

        let path_meta = self.parse_metadata_from_path(parent_path, meta.is_tv_episode());
        meta.merge_metadata(&path_meta);
        meta
    }

    pub(crate) fn build_media_files(
        &mut self,
        raw_files: Vec<RawFile>,
        descriptions: Vec<String>,
    ) -> Vec<MediaFile> {
        let mut parsed_files = Vec::new();

        for raw_file in raw_files {
            let metadata = self.parse_media_metadata(&raw_file.name, &raw_file.path);
            if metadata.unknown_type() {
                continue;
            }

            parsed_files.push((metadata, raw_file));
        }

        group_video_and_subtitle_files(parsed_files, descriptions)
    }
}

#[cfg(test)]
#[path = "metadata/tests.rs"]
mod tests;
