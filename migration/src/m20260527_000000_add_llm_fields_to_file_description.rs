use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(FileDescription::Table)
                    .add_column(
                        ColumnDef::new(FileDescription::ExtractedTitle)
                            .text()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(FileDescription::Table)
                    .add_column(
                        ColumnDef::new(FileDescription::ExtractedLanguage)
                            .text()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(FileDescription::Table)
                    .drop_column(FileDescription::ExtractedTitle)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(FileDescription::Table)
                    .drop_column(FileDescription::ExtractedLanguage)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum FileDescription {
    Table,
    ExtractedTitle,
    ExtractedLanguage,
}
