use crate::domain::{import::inner::MediaFile, share::RawFile};

pub(in crate::application::import) fn build_local_cleanup_paths(
    local_parent_path: &str,
    media_file: &MediaFile,
) -> Vec<String> {
    let mut paths = vec![format!(
        "{}/{}.strm",
        local_parent_path,
        media_file
            .video
            .name
            .trim_end_matches(media_file.metadata.extension.as_str())
    )];
    paths.extend(
        media_file
            .subtitles
            .iter()
            .map(|subtitle| format!("{}/{}", local_parent_path, subtitle.name)),
    );
    paths
}

pub(in crate::application::import) fn renamed_subtitle_file_name(
    raw_file: &RawFile,
    source_video_name: &str,
    target_video_name: &str,
    extension: &str,
) -> String {
    raw_file.name.replace(
        source_video_name.trim_end_matches(extension),
        target_video_name.trim_end_matches(extension),
    )
}

pub(in crate::application::import) fn remote_child_path(
    parent_path: &str,
    file_name: &str,
) -> String {
    format!("{}/{}", parent_path, file_name)
}

pub(in crate::application::import) fn build_subtitle_transfer_plan<'a>(
    media_file: &'a MediaFile,
    video_file_name: &str,
) -> Vec<(&'a RawFile, String)> {
    media_file
        .subtitles
        .iter()
        .map(|subtitle| {
            (
                subtitle,
                renamed_subtitle_file_name(
                    subtitle,
                    &media_file.video.name,
                    video_file_name,
                    media_file.metadata.extension.as_str(),
                ),
            )
        })
        .collect()
}
