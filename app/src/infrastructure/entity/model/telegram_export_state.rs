//! `SeaORM` Entity for telegram_export_state.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "telegram_export_state")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub source_type: String,
    #[sea_orm(column_type = "Text")]
    pub source_value: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    pub status: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub error: Option<String>,
    pub attempt_count: i64,
    pub first_seen_at: String,
    pub last_attempt_at: String,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
