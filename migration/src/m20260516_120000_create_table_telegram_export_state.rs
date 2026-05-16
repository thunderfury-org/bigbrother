use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TelegramExportState::Table)
                    .if_not_exists()
                    .col(pk_auto(TelegramExportState::Id))
                    .col(string(TelegramExportState::SourceType))
                    .col(text(TelegramExportState::SourceValue))
                    .col(text_null(TelegramExportState::Description))
                    .col(string(TelegramExportState::Status))
                    .col(text_null(TelegramExportState::Error))
                    .col(big_integer(TelegramExportState::AttemptCount))
                    .col(string(TelegramExportState::FirstSeenAt))
                    .col(string(TelegramExportState::LastAttemptAt))
                    .col(timestamp(TelegramExportState::CreateTime))
                    .col(timestamp(TelegramExportState::UpdateTime))
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-telegram-export-state-source")
                            .col(TelegramExportState::SourceType)
                            .col(TelegramExportState::SourceValue),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(TelegramExportState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum TelegramExportState {
    Table,
    Id,
    SourceType,
    SourceValue,
    Description,
    Status,
    Error,
    AttemptCount,
    FirstSeenAt,
    LastAttemptAt,
    CreateTime,
    UpdateTime,
}
