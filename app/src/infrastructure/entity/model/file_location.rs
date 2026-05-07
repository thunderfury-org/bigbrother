//! `SeaORM` Entity for file_location.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_location")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub file_index_id: i64,
    #[sea_orm(column_type = "Text")]
    pub file_name: String,
    #[sea_orm(column_type = "Text")]
    pub file_path: String,
    pub location_hash: String,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
