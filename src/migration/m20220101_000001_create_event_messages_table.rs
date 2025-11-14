use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EventMessages::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EventMessages::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(EventMessages::Topic).string().not_null())
                    .col(ColumnDef::new(EventMessages::Message).string().not_null())
                    .col(
                        ColumnDef::new(EventMessages::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EventMessages::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum EventMessages {
    Table,
    Id,
    Topic,
    Message,
    CreatedAt,
}
