use crate::domain::media::Metadata;

use super::MetadataLookup;

impl MetadataLookup {
    pub(super) fn parse_metadata_from_path(
        &mut self,
        parent_path: &str,
        is_tv: bool,
    ) -> Box<Metadata> {
        if let Some(meta) = self.cache.get(parent_path) {
            return meta.clone();
        }

        let parts = parent_path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Box::new(Metadata::default());
        }

        let mut meta = Metadata::parse(parts[parts.len() - 1]);
        if !is_tv || parts.len() < 2 {
            self.cache.insert(parent_path.to_string(), meta.clone());
            return meta;
        }

        let path_meta = Metadata::parse(parts[parts.len() - 2]);
        meta.merge_metadata(&path_meta);

        self.cache.insert(parent_path.to_string(), meta.clone());
        meta
    }
}
