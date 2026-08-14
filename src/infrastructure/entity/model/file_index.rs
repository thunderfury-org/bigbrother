//! `SeaORM` Entity for file_index.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_index")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub size: i64,
    pub hash_type: String,
    pub hash_value: String,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
