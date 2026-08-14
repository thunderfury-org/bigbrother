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
    async fn get(
        &self,
        source_type: &str,
        source_value: &str,
    ) -> AppResult<Option<TelegramExportStateRecord>> {
        Ok(entity::telegram_export_state::get(&self.db, source_type, source_value).await?)
    }

    async fn upsert(&self, record: &TelegramExportStateRecord) -> AppResult<()> {
        entity::telegram_export_state::upsert(&self.db, record).await?;
        Ok(())
    }
}
