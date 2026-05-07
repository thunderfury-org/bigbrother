#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Genre {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MovieDetail {
    pub id: u32,
    pub title: String,
    pub adult: bool,
    pub genres: Vec<Genre>,
    pub original_language: String,
    pub original_title: String,
    pub origin_country: Vec<String>,
    pub release_date: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SearchMovieResult {
    pub id: u32,
    pub title: String,
    pub original_title: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Season {
    pub id: u32,
    pub name: String,
    pub episode_count: u32,
    pub season_number: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TvDetail {
    pub id: u32,
    pub name: String,
    pub first_air_date: String,
    pub number_of_episodes: u32,
    pub number_of_seasons: u32,
    pub origin_country: Vec<String>,
    pub original_language: String,
    pub original_name: String,
    pub genres: Vec<Genre>,
    pub seasons: Vec<Season>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SearchTvResult {
    pub id: u32,
    pub name: String,
    pub original_name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct LibraryFile {
    pub file_id: i64,
    pub file_name: String,
    pub is_dir: bool,
    pub size: u64,
    pub etag: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pan189ShareInfo {
    pub file_id: String,
    pub file_name: String,
    pub share_id: i64,
    pub share_mode: i32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pan189Folder {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pan189File {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub md5: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pan115FileEntry {
    pub fid: Option<String>,
    pub cid: Option<String>,
    pub name: String,
    pub size: u64,
    pub sha: Option<String>,
}

impl Pan115FileEntry {
    pub fn is_file(&self) -> bool {
        self.fid.is_some()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct QuarkShareInfo {
    pub stoken: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct QuarkFolder {
    pub fid: String,
    pub name: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct QuarkFile {
    pub fid: String,
    pub name: String,
    pub size: u64,
    pub share_fid_token: String,
}
