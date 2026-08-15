use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ImportRecord::Table)
                    .if_not_exists()
                    .col(pk_auto(ImportRecord::Id))
                    .col(string(ImportRecord::SourceKind))
                    .col(text(ImportRecord::Source))
                    .col(string(ImportRecord::Status))
                    .col(text_null(ImportRecord::SummaryJson))
                    .col(string_null(ImportRecord::ErrorKind))
                    .col(text_null(ImportRecord::ErrorMessage))
                    .col(timestamp(ImportRecord::CreatedAt))
                    .col(timestamp(ImportRecord::UpdatedAt))
                    .col(timestamp_null(ImportRecord::FinishedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(ImportRecord::Table)
                    .name("idx-import-record-status-created-at")
                    .col(ImportRecord::Status)
                    .col(ImportRecord::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(ImportRecord::Table)
                    .name("idx-import-record-created-at")
                    .col(ImportRecord::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(ImportRecord::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ImportRecord {
    Table,
    Id,
    SourceKind,
    Source,
    Status,
    SummaryJson,
    ErrorKind,
    ErrorMessage,
    CreatedAt,
    UpdatedAt,
    FinishedAt,
}
