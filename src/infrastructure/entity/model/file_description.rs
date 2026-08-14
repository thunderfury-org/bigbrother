//! `SeaORM` Entity for file_description.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_description")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub content_hash: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub create_time: DateTimeUtc,
    pub extracted_title: Option<String>,
    pub extracted_language: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
