use crate::{
    domain::import::{MovieDetail, TvDetail},
    domain::library::import_paths,
};

pub(crate) fn get_tv_path_in_library(remote_path: &str, tv: &TvDetail) -> String {
    let genre_ids = tv.genres.iter().map(|genre| genre.id).collect::<Vec<_>>();
    import_paths::get_tv_path_in_library(
        remote_path,
        &genre_ids,
        &tv.origin_country,
        &tv.name,
        tv.first_air_date.as_str(),
        tv.id,
    )
}

pub(crate) fn get_movie_path_in_library(remote_path: &str, movie: &MovieDetail) -> String {
    import_paths::get_movie_path_in_library(
        remote_path,
        &movie.origin_country,
        &movie.title,
        movie.release_date.as_str(),
        movie.id,
    )
}

pub(crate) fn get_tv_base_name(tv: &TvDetail) -> String {
    import_paths::get_tv_base_name(&tv.name, tv.first_air_date.as_str(), tv.id)
}

pub(crate) fn get_movie_base_name(movie: &MovieDetail) -> String {
    import_paths::get_movie_base_name(&movie.title, movie.release_date.as_str(), movie.id)
}

pub(crate) fn get_year_from_date(date: &str) -> &str {
    import_paths::get_year_from_date(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_year_from_date() {
        assert_eq!(get_year_from_date("2024-01-15"), "2024");
        assert_eq!(get_year_from_date("invalid-date"), "invalid");
    }

    #[test]
    fn test_get_movie_base_name() {
        let movie = MovieDetail {
            id: 1,
            title: "Inception".into(),
            release_date: "2010-07-16".into(),
            ..Default::default()
        };
        assert_eq!(get_movie_base_name(&movie), "Inception (2010) {tmdb-1}");
    }

    #[test]
    fn test_get_tv_path_in_library() {
        let tv = TvDetail {
            id: 1,
            name: "Show".into(),
            first_air_date: "2020-01-01".into(),
            genres: vec![Genre {
                id: 16,
                name: "Animation".into(),
            }],
            origin_country: vec!["JP".into()],
            ..Default::default()
        };
        assert_eq!(
            get_tv_path_in_library("/media", &tv),
            "/media/动漫/日韩/Show (2020) {tmdb-1}"
        );
    }
}
