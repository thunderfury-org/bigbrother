use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Subscription::Table)
                    .if_not_exists()
                    .col(pk_auto(Subscription::Id))
                    .col(integer(Subscription::TmdbId))
                    .col(string(Subscription::MediaType))
                    .col(string_null(Subscription::TitleZh))
                    .col(string_null(Subscription::TitleEn))
                    .col(timestamp(Subscription::CreateTime))
                    .col(timestamp(Subscription::UpdateTime))
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-subscription-tmdb-id-media-type")
                            .col(Subscription::TmdbId)
                            .col(Subscription::MediaType),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Subscription::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Subscription {
    Table,
    Id,
    TmdbId,
    MediaType,
    TitleZh,
    TitleEn,
    CreateTime,
    UpdateTime,
}
