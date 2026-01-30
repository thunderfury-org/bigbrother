use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Cache::Table)
                    .if_not_exists()
                    .col(pk_auto(Cache::Id))
                    .col(string(Cache::Key))
                    .col(text(Cache::Value))
                    .col(timestamp_null(Cache::ExpiredAt))
                    .col(timestamp(Cache::CreateTime))
                    .col(timestamp(Cache::UpdateTime))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(Cache::Table)
                    .name("idx-cache-key")
                    .unique()
                    .col(Cache::Key)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(Cache::Table)
                    .name("idx-cache-expired-at")
                    .col(Cache::ExpiredAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Cache::Table).to_owned()).await
    }
}

#[derive(DeriveIden)]
enum Cache {
    Table,
    Id,
    Key,
    Value,
    ExpiredAt,
    CreateTime,
    UpdateTime,
}
