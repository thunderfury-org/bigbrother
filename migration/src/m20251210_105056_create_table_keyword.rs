use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Keyword::Table)
                    .if_not_exists()
                    .col(pk_auto(Keyword::Id))
                    .col(string(Keyword::Value).not_null())
                    .col(timestamp(Keyword::CreateTime).not_null())
                    .index(Index::create().unique().name("idx-keyword-value").col(Keyword::Value))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Keyword::Table).to_owned()).await
    }
}

#[derive(DeriveIden)]
enum Keyword {
    Table,
    Id,
    Value,
    CreateTime,
}
