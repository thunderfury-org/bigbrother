use std::{collections::HashMap, sync::LazyLock};

const GENRE_ANIMATION: u32 = 16;
const GENRE_DOCUMENTARY: u32 = 99;
const GENRE_REALITY: u32 = 10764;
const GENRE_TALK: u32 = 10767;

pub const CATEGORY_MOVIE: &str = "电影";
const CATEGORY_TV: &str = "电视剧";
const CATEGORY_ANIMATION: &str = "动漫";
const CATEGORY_DOCUMENTARY: &str = "纪录片";
const CATEGORY_VARIETY: &str = "综艺";

const SUBCATEGORY_OTHER: &str = "其它";

static SUB_CATALOG: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let mapping = vec![
        ("国产", vec!["CN", "TW", "HK"]),
        ("日韩", vec!["JP", "KP", "KR", "TH", "IN", "SG"]),
        (
            "欧美",
            vec!["US", "FR", "GB", "DE", "ES", "IT", "NL", "PT", "RU", "UK"],
        ),
    ];
    for (k, v) in mapping {
        for c in v {
            m.insert(c, k);
        }
    }
    m
});

pub fn get_tv_category(genre_ids: &[u32]) -> &'static str {
    for genre_id in genre_ids {
        match *genre_id {
            GENRE_ANIMATION => return CATEGORY_ANIMATION,
            GENRE_DOCUMENTARY => return CATEGORY_DOCUMENTARY,
            GENRE_REALITY | GENRE_TALK => return CATEGORY_VARIETY,
            _ => {}
        }
    }
    CATEGORY_TV
}

pub fn get_subcategory(original_country: &[String]) -> &'static str {
    for country in original_country {
        let country = country.to_uppercase();
        if let Some(category) = SUB_CATALOG.get(country.as_str()) {
            return category;
        }
    }
    SUBCATEGORY_OTHER
}

pub fn get_year_from_date(date: &str) -> &str {
    date.split('-').next().unwrap_or_default()
}

pub fn get_tv_base_name(name: &str, first_air_date: &str, tmdb_id: u32) -> String {
    format!(
        "{} ({}) {{tmdb-{}}}",
        name,
        get_year_from_date(first_air_date),
        tmdb_id
    )
}

pub fn get_movie_base_name(title: &str, release_date: &str, tmdb_id: u32) -> String {
    format!(
        "{} ({}) {{tmdb-{}}}",
        title,
        get_year_from_date(release_date),
        tmdb_id
    )
}

pub fn get_tv_path_in_library(
    remote_path: &str,
    genre_ids: &[u32],
    origin_country: &[String],
    name: &str,
    first_air_date: &str,
    tmdb_id: u32,
) -> String {
    format!(
        "{}/{}/{}/{}",
        remote_path,
        get_tv_category(genre_ids),
        get_subcategory(origin_country),
        get_tv_base_name(name, first_air_date, tmdb_id)
    )
}

pub fn get_movie_path_in_library(
    remote_path: &str,
    origin_country: &[String],
    title: &str,
    release_date: &str,
    tmdb_id: u32,
) -> String {
    format!(
        "{}/{}/{}/{}",
        remote_path,
        CATEGORY_MOVIE,
        get_subcategory(origin_country),
        get_movie_base_name(title, release_date, tmdb_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tv_category_animation() {
        assert_eq!(get_tv_category(&[GENRE_ANIMATION]), CATEGORY_ANIMATION);
    }

    #[test]
    fn test_get_tv_category_documentary() {
        assert_eq!(get_tv_category(&[GENRE_DOCUMENTARY]), CATEGORY_DOCUMENTARY);
    }

    #[test]
    fn test_get_tv_category_reality() {
        assert_eq!(get_tv_category(&[GENRE_REALITY]), CATEGORY_VARIETY);
    }

    #[test]
    fn test_get_tv_category_talk() {
        assert_eq!(get_tv_category(&[GENRE_TALK]), CATEGORY_VARIETY);
    }

    #[test]
    fn test_get_tv_category_default() {
        assert_eq!(get_tv_category(&[1]), CATEGORY_TV);
    }

    #[test]
    fn test_get_tv_category_empty() {
        assert_eq!(get_tv_category(&[]), CATEGORY_TV);
    }

    #[test]
    fn test_get_subcategory_case_insensitive() {
        assert_eq!(get_subcategory(&["cn".to_string()]), "国产");
    }

    #[test]
    fn test_get_subcategory_skips_unknown_countries() {
        assert_eq!(
            get_subcategory(&["XX".to_string(), "CN".to_string()]),
            "国产"
        );
    }

    #[test]
    fn test_get_year_from_date() {
        assert_eq!(get_year_from_date("2024-01-15"), "2024");
        assert_eq!(get_year_from_date("invalid-date"), "invalid");
        assert_eq!(get_year_from_date(""), "");
    }

    #[test]
    fn test_get_tv_base_name() {
        assert_eq!(
            get_tv_base_name("Breaking Bad", "2008-01-20", 12345),
            "Breaking Bad (2008) {tmdb-12345}"
        );
    }

    #[test]
    fn test_get_movie_base_name() {
        assert_eq!(
            get_movie_base_name("The Matrix", "1999-03-31", 98765),
            "The Matrix (1999) {tmdb-98765}"
        );
    }

    #[test]
    fn test_get_tv_path_in_library() {
        let path = get_tv_path_in_library(
            "/media",
            &[GENRE_ANIMATION],
            &["JP".to_string()],
            "进击的巨人",
            "2013-04-07",
            12345,
        );
        assert_eq!(path, "/media/动漫/日韩/进击的巨人 (2013) {tmdb-12345}");
    }

    #[test]
    fn test_get_movie_path_in_library() {
        let path = get_movie_path_in_library(
            "/media",
            &["US".to_string()],
            "Inception",
            "2010-07-16",
            27205,
        );
        assert_eq!(path, "/media/电影/欧美/Inception (2010) {tmdb-27205}");
    }
}
