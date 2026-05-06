//! `SeaORM` Entity for file_location_description.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_location_description")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub file_location_id: i64,
    pub file_description_id: i64,
    pub create_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
