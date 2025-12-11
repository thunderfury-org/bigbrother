use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MessageFilter::Table)
                    .if_not_exists()
                    .col(pk_auto(MessageFilter::Id))
                    .col(string(MessageFilter::Filter).not_null())
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-message_filter-filter")
                            .col(MessageFilter::Filter),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MessageFilter::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MessageFilter {
    Table,
    Id,
    Filter,
}
