#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPathMapper {
    remote_root: String,
    local_root: String,
}

impl SyncPathMapper {
    pub fn new(remote_root: impl Into<String>, local_root: impl Into<String>) -> Self {
        Self {
            remote_root: remote_root.into(),
            local_root: local_root.into(),
        }
    }

    pub fn remote_to_local_path(&self, remote_path: &str) -> String {
        remote_path.replacen(self.remote_root.as_str(), self.local_root.as_str(), 1)
    }

    pub fn remote_to_local_strm_path(&self, remote_file_path: &str, extension: &str) -> String {
        self.remote_to_local_path(remote_file_path)
            .trim_end_matches(extension)
            .to_owned()
            + ".strm"
    }
}

#[cfg(test)]
mod tests {
    use super::SyncPathMapper;

    #[test]
    fn rewrites_remote_path_prefix() {
        let mapper = SyncPathMapper::new("/remote", "/local");

        assert_eq!(
            mapper.remote_to_local_path("/remote/tv/show.srt"),
            "/local/tv/show.srt"
        );
    }

    #[test]
    fn rewrites_video_extension_to_strm() {
        let mapper = SyncPathMapper::new("/remote", "/local");

        assert_eq!(
            mapper.remote_to_local_strm_path("/remote/tv/ep01.mkv", ".mkv"),
            "/local/tv/ep01.strm"
        );
    }
}
