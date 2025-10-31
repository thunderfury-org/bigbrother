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

pub fn get_tv_category(genres: &Vec<Genre>) -> &'static str {
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

pub fn get_subcategory(original_country: &Vec<String>) -> &'static str {
    for c in original_country {
        let c = c.to_uppercase();
        if let Some(k) = SUB_CATALOG.get(c.as_str()) {
            return k;
        }
    }
    SUBCATEGORY_OTHER
}
