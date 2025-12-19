use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Event::Table)
                    .if_not_exists()
                    .col(pk_auto(Event::Id))
                    .col(string(Event::Event))
                    .col(text(Event::Payload))
                    .col(boolean(Event::Ack))
                    .col(timestamp(Event::CreateTime))
                    .col(timestamp(Event::UpdateTime))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(Event::Table)
                    .name("idx-event-ack")
                    .col(Event::Event)
                    .col(Event::Ack)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Event::Table).to_owned()).await
    }
}

#[derive(DeriveIden)]
enum Event {
    Table,
    Id,
    Event,
    Payload,
    Ack,
    CreateTime,
    UpdateTime,
}
