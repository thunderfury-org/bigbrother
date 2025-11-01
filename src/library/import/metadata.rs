use crate::media::Metadata;

use super::Importer;

impl Importer {
    pub(super) fn parse_media_metadata(&self, name: &str, path: &str) -> Metadata {
        // todo: try to parse metadata from path
        Metadata::from(name)
    }
}
