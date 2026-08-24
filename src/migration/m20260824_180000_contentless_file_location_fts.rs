use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS file_location_fts")
            .await?;
        db.execute_unprepared(
            r#"
            CREATE VIRTUAL TABLE file_location_fts USING fts5(
                file_name,
                file_path,
                description,
                tokenize = 'unicode61',
                content = '',
                contentless_delete = 1
            );
            "#,
        )
        .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS `idx-file-location-name`")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS `idx-file-location-path`")
            .await?;
        db.execute_unprepared("VACUUM").await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS file_location_fts")
            .await?;
        db.execute_unprepared(
            r#"
            CREATE VIRTUAL TABLE file_location_fts USING fts5(
                file_name,
                file_path,
                description,
                tokenize = 'unicode61'
            );
            "#,
        )
        .await?;
        db.execute_unprepared(
            r#"
            CREATE INDEX IF NOT EXISTS `idx-file-location-name`
            ON file_location (file_name);
            "#,
        )
        .await?;
        db.execute_unprepared(
            r#"
            CREATE INDEX IF NOT EXISTS `idx-file-location-path`
            ON file_location (file_path);
            "#,
        )
        .await?;
        Ok(())
    }
}
