use crate::media::Metadata;

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

        let parts = parent_path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
        if parts.is_empty() {
            return Box::new(Metadata::default());
        }

        let mut meta = Metadata::parse(parts.last().unwrap());
        if !is_tv || parts.len() < 2 {
            self.metadata_cache.insert(parent_path.to_string(), meta.clone());
            return meta;
        }

        let path_meta = Metadata::parse(parts[parts.len() - 2]);
        meta.merge_metadata(&path_meta);

        self.metadata_cache.insert(parent_path.to_string(), meta.clone());
        meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_media_metadata() {
        let mut importer = Importer::new_test();
        let meta = importer.parse_media_metadata(
            "唐朝诡事录 - 2025 - S03E11 - HDR - 第 11 集.mkv",
            "唐朝诡事录（2022）{tmdb-211089}//Season 3",
        );
        assert_eq!(meta.titles.len(), 1);
        assert_eq!(meta.titles[0].title, "唐朝诡事录");
        assert_eq!(meta.year, "2022");
        assert_eq!(meta.tmdb_id, "211089");
        assert_eq!(meta.season_number, Some(3));
        assert_eq!(meta.episode_number, Some(11));
        assert_eq!(meta.resolution, "");
        assert_eq!(meta.frame_rate, "");
        assert_eq!(meta.quality, "");
        assert_eq!(meta.hdr, "HDR");
        assert_eq!(meta.video_codec, "");
        assert_eq!(meta.audio_codec, "");
    }

    #[test]
    fn test_parse_media_metadata_1() {
        let mut importer = Importer::new_test();
        let meta = importer.parse_media_metadata(
            "S02E32.2024.2160p.WEB-DL.H265.EDR.DDP5.1-BestWEB.mkv",
            "唐朝诡事录（2022）{tmdb-211089}//Season 3",
        );
        assert_eq!(meta.titles.len(), 1);
        assert_eq!(meta.titles[0].title, "唐朝诡事录");
        assert_eq!(meta.year, "2022");
        assert_eq!(meta.tmdb_id, "211089");
        assert_eq!(meta.season_number, Some(2));
        assert_eq!(meta.episode_number, Some(32));
        assert_eq!(meta.resolution, "2160p");
        assert_eq!(meta.frame_rate, "");
        assert_eq!(meta.quality, "WEB-DL");
        assert_eq!(meta.hdr, "");
        assert_eq!(meta.video_codec, "H265");
        assert_eq!(meta.audio_codec, "DDP.5.1");
        assert_eq!(meta.release_group, "BestWEB");
    }
}
