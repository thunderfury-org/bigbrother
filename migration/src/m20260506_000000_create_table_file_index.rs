use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FileIndex::Table)
                    .if_not_exists()
                    .col(pk_auto(FileIndex::Id))
                    .col(big_unsigned(FileIndex::Size))
                    .col(string_null(FileIndex::Md5))
                    .col(string_null(FileIndex::Sha1))
                    .col(timestamp(FileIndex::CreateTime))
                    .col(timestamp(FileIndex::UpdateTime))
                    .check(Expr::cust("md5 IS NOT NULL OR sha1 IS NOT NULL"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .unique()
                    .name("idx-file-index-size-md5")
                    .table(FileIndex::Table)
                    .col(FileIndex::Size)
                    .col(FileIndex::Md5)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .unique()
                    .name("idx-file-index-size-sha1")
                    .table(FileIndex::Table)
                    .col(FileIndex::Size)
                    .col(FileIndex::Sha1)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileLocation::Table)
                    .if_not_exists()
                    .col(pk_auto(FileLocation::Id))
                    .col(big_integer(FileLocation::FileIndexId))
                    .col(text(FileLocation::FileName))
                    .col(text(FileLocation::FilePath))
                    .col(string(FileLocation::LocationHash))
                    .col(timestamp(FileLocation::CreateTime))
                    .col(timestamp(FileLocation::UpdateTime))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-file-index")
                            .from(FileLocation::Table, FileLocation::FileIndexId)
                            .to(FileIndex::Table, FileIndex::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-location-file-hash")
                            .col(FileLocation::FileIndexId)
                            .col(FileLocation::LocationHash),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-file-location-name")
                    .table(FileLocation::Table)
                    .col(FileLocation::FileName)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-file-location-path")
                    .table(FileLocation::Table)
                    .col(FileLocation::FilePath)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileDescription::Table)
                    .if_not_exists()
                    .col(pk_auto(FileDescription::Id))
                    .col(string(FileDescription::ContentHash))
                    .col(text(FileDescription::Description))
                    .col(timestamp(FileDescription::CreateTime))
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-description-hash")
                            .col(FileDescription::ContentHash),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileLocationDescription::Table)
                    .if_not_exists()
                    .col(pk_auto(FileLocationDescription::Id))
                    .col(big_integer(FileLocationDescription::FileLocationId))
                    .col(big_integer(FileLocationDescription::FileDescriptionId))
                    .col(timestamp(FileLocationDescription::CreateTime))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-description-location")
                            .from(
                                FileLocationDescription::Table,
                                FileLocationDescription::FileLocationId,
                            )
                            .to(FileLocation::Table, FileLocation::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-description-description")
                            .from(
                                FileLocationDescription::Table,
                                FileLocationDescription::FileDescriptionId,
                            )
                            .to(FileDescription::Table, FileDescription::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-location-description-link")
                            .col(FileLocationDescription::FileLocationId)
                            .col(FileLocationDescription::FileDescriptionId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(FileLocationDescription::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileDescription::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileLocation::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(FileIndex::Table).if_exists().to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum FileIndex {
    Table,
    Id,
    Size,
    Md5,
    Sha1,
    CreateTime,
    UpdateTime,
}

#[derive(DeriveIden)]
enum FileLocation {
    Table,
    Id,
    FileIndexId,
    FileName,
    FilePath,
    LocationHash,
    CreateTime,
    UpdateTime,
}

#[derive(DeriveIden)]
enum FileDescription {
    Table,
    Id,
    ContentHash,
    Description,
    CreateTime,
}

#[derive(DeriveIden)]
enum FileLocationDescription {
    Table,
    Id,
    FileLocationId,
    FileDescriptionId,
    CreateTime,
}
