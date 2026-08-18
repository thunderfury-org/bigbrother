use sea_orm::entity::prelude::*;

#[allow(dead_code)]
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub tmdb_id: i32,
    pub media_type: String,
    pub title_zh: Option<String>,
    pub title_en: Option<String>,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
