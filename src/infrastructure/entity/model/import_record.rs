//! `SeaORM` Entity for import_record.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "import_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub source_kind: String,
    #[sea_orm(column_type = "Text")]
    pub source: String,
    pub status: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub summary_json: Option<String>,
    #[sea_orm(nullable)]
    pub error_kind: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub error_message: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(nullable)]
    pub finished_at: Option<DateTimeUtc>,
}

impl ActiveModelBehavior for ActiveModel {}
