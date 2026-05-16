use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{TelegramExportStateRecord, TelegramExportStateRepository},
    error::AppResult,
    infrastructure::entity,
};

#[derive(Clone)]
pub struct SeaOrmTelegramExportStateRepository {
    db: DatabaseConnection,
}

impl SeaOrmTelegramExportStateRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl TelegramExportStateRepository for SeaOrmTelegramExportStateRepository {
    async fn list_all(&self) -> AppResult<Vec<TelegramExportStateRecord>> {
        Ok(entity::telegram_export_state::list_all(&self.db).await?)
    }

    async fn upsert(&self, record: &TelegramExportStateRecord) -> AppResult<()> {
        entity::telegram_export_state::upsert(&self.db, record).await?;
        Ok(())
    }
}
