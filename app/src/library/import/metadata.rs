use crate::domain::media::Metadata;

use super::Importer;

impl Importer {
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
        if let Some(meta) = self.metadata_cache.get(parent_path) {
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
            self.metadata_cache
                .insert(parent_path.to_string(), meta.clone());
            return meta;
        }

        let path_meta = Metadata::parse(parts[parts.len() - 2]);
        meta.merge_metadata(&path_meta);

        self.metadata_cache
            .insert(parent_path.to_string(), meta.clone());
        meta
    }
}
