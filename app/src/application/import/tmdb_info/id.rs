use tracing::warn;

use crate::domain::media::Metadata;

pub(super) fn parsed_tmdb_id(meta: &Metadata, media_type: &str) -> Option<u32> {
    if meta.tmdb_id.is_empty() {
        return None;
    }

    match meta.tmdb_id.parse() {
        Ok(tmdb_id) => Some(tmdb_id),
        Err(error) => {
            warn!(
                "Invalid {} tmdb id '{}', title candidates: {:?}, error: {}",
                media_type,
                meta.tmdb_id,
                meta.titles
                    .iter()
                    .map(|title| &title.title)
                    .collect::<Vec<_>>(),
                error
            );
            None
        }
    }
}
