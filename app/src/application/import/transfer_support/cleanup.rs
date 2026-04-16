use std::collections::HashMap;

use crate::domain::import::{inner::MediaFile, policy::collect_replaced_media_files};

pub(in crate::application::import) fn files_pending_cleanup<'a>(
    existing_files: Option<&'a [MediaFile]>,
    saved_filename: Option<&str>,
) -> Vec<&'a MediaFile> {
    let Some(existing_files) = existing_files else {
        return Vec::new();
    };
    let Some(saved_filename) = saved_filename else {
        return Vec::new();
    };

    collect_replaced_media_files(existing_files, &Some(saved_filename.to_string()))
}

pub(in crate::application::import) fn collect_library_file_ids(files: &[&MediaFile]) -> Vec<i64> {
    let mut file_ids = Vec::new();

    for media_file in files {
        if let Some(id) = media_file.video.id {
            file_ids.push(id);
        }
        file_ids.extend(
            media_file
                .subtitles
                .iter()
                .filter_map(|subtitle| subtitle.id),
        );
    }

    file_ids
}

pub(in crate::application::import) fn existing_season_dir_id(
    season_dir: &str,
    season_dir_ids: &HashMap<String, i64>,
) -> Option<i64> {
    season_dir_ids.get(season_dir).copied()
}
