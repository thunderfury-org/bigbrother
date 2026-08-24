use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(FileLocationDescription::Table)
                    .name("idx-file-location-description-description-id")
                    .col(FileLocationDescription::FileDescriptionId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(FileLocationDescription::Table)
                    .name("idx-file-location-description-description-id")
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum FileLocationDescription {
    Table,
    FileDescriptionId,
}
