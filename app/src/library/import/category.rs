use std::{collections::HashMap, sync::LazyLock};

use crate::client::tmdb::Genre;

const GENRE_ANIMATION: u32 = 16; // 动漫
const GENRE_DOCUMENTARY: u32 = 99; // 记录
const GENRE_REALITY: u32 = 10764; // 真人秀
const GENRE_TALK: u32 = 10767; // 脱口秀

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
        ("欧美", vec!["US", "FR", "GB", "DE", "ES", "IT", "NL", "PT", "RU", "UK"]),
    ];
    for (k, v) in mapping {
        for c in v {
            m.insert(c, k);
        }
    }
    m
});

pub(super) fn get_tv_category(genres: &Vec<Genre>) -> &'static str {
    for g in genres {
        match g.id {
            GENRE_ANIMATION => return CATEGORY_ANIMATION,
            GENRE_DOCUMENTARY => return CATEGORY_DOCUMENTARY,
            GENRE_REALITY | GENRE_TALK => return CATEGORY_VARIETY,
            _ => {}
        }
    }
    CATEGORY_TV
}

pub(super) fn get_subcategory(original_country: &Vec<String>) -> &'static str {
    for c in original_country {
        let c = c.to_uppercase();
        if let Some(k) = SUB_CATALOG.get(c.as_str()) {
            return k;
        }
    }
    SUBCATEGORY_OTHER
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for get_tv_category
    #[test]
    fn test_get_tv_category_animation() {
        let genres = vec![Genre {
            id: GENRE_ANIMATION,
            name: "Animation".to_string(),
        }];
        assert_eq!(get_tv_category(&genres), CATEGORY_ANIMATION);
    }

    #[test]
    fn test_get_tv_category_documentary() {
        let genres = vec![Genre {
            id: GENRE_DOCUMENTARY,
            name: "Documentary".to_string(),
        }];
        assert_eq!(get_tv_category(&genres), CATEGORY_DOCUMENTARY);
    }

    #[test]
    fn test_get_tv_category_reality() {
        let genres = vec![Genre {
            id: GENRE_REALITY,
            name: "Reality".to_string(),
        }];
        assert_eq!(get_tv_category(&genres), CATEGORY_VARIETY);
    }

    #[test]
    fn test_get_tv_category_talk() {
        let genres = vec![Genre {
            id: GENRE_TALK,
            name: "Talk".to_string(),
        }];
        assert_eq!(get_tv_category(&genres), CATEGORY_VARIETY);
    }

    #[test]
    fn test_get_tv_category_default() {
        let genres = vec![Genre {
            id: 1,
            name: "Other".to_string(),
        }];
        assert_eq!(get_tv_category(&genres), CATEGORY_TV);
    }

    #[test]
    fn test_get_tv_category_empty() {
        let genres = vec![];
        assert_eq!(get_tv_category(&genres), CATEGORY_TV);
    }

    #[test]
    fn test_get_tv_category_first_match_wins() {
        let genres = vec![
            Genre {
                id: 1,
                name: "Other".to_string(),
            },
            Genre {
                id: GENRE_ANIMATION,
                name: "Animation".to_string(),
            },
        ];
        assert_eq!(get_tv_category(&genres), CATEGORY_ANIMATION);
    }

    #[test]
    fn test_get_tv_category_animation_takes_precedence() {
        let genres = vec![
            Genre {
                id: GENRE_DOCUMENTARY,
                name: "Documentary".to_string(),
            },
            Genre {
                id: GENRE_ANIMATION,
                name: "Animation".to_string(),
            },
        ];
        assert_eq!(get_tv_category(&genres), CATEGORY_DOCUMENTARY);
    }

    // Tests for get_subcategory
    #[test]
    fn test_get_subcategory_china() {
        let country = vec!["CN".to_string()];
        assert_eq!(get_subcategory(&country), "国产");
    }

    #[test]
    fn test_get_subcategory_taiwan() {
        let country = vec!["TW".to_string()];
        assert_eq!(get_subcategory(&country), "国产");
    }

    #[test]
    fn test_get_subcategory_hongkong() {
        let country = vec!["HK".to_string()];
        assert_eq!(get_subcategory(&country), "国产");
    }

    #[test]
    fn test_get_subcategory_japan() {
        let country = vec!["JP".to_string()];
        assert_eq!(get_subcategory(&country), "日韩");
    }

    #[test]
    fn test_get_subcategory_korea() {
        let country = vec!["KR".to_string()];
        assert_eq!(get_subcategory(&country), "日韩");
    }

    #[test]
    fn test_get_subcategory_usa() {
        let country = vec!["US".to_string()];
        assert_eq!(get_subcategory(&country), "欧美");
    }

    #[test]
    fn test_get_subcategory_europe() {
        let country = vec!["GB".to_string()];
        assert_eq!(get_subcategory(&country), "欧美");
    }

    #[test]
    fn test_get_subcategory_unknown() {
        let country = vec!["XX".to_string()];
        assert_eq!(get_subcategory(&country), SUBCATEGORY_OTHER);
    }

    #[test]
    fn test_get_subcategory_empty() {
        let country = vec![];
        assert_eq!(get_subcategory(&country), SUBCATEGORY_OTHER);
    }

    #[test]
    fn test_get_subcategory_multiple_countries() {
        let country = vec!["US".to_string(), "CN".to_string()];
        assert_eq!(get_subcategory(&country), "欧美");
    }

    #[test]
    fn test_get_subcategory_case_insensitive() {
        let country = vec!["cn".to_string()];
        assert_eq!(get_subcategory(&country), "国产");
    }

    #[test]
    fn test_get_subcategory_skips_unknown_countries() {
        let country = vec!["XX".to_string(), "CN".to_string()];
        assert_eq!(get_subcategory(&country), "国产");
    }
}
