use sea_orm::DatabaseConnection;

use crate::{error::AppResult, infrastructure::entity::event};

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: i64,
    pub payload: String,
}

#[derive(Clone)]
pub struct SeaOrmEventStore {
    db: DatabaseConnection,
}

impl SeaOrmEventStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn append(&self, name: &str, payload: &str) -> AppResult<()> {
        event::add_record(&self.db, name, payload).await?;
        Ok(())
    }

    pub async fn list_pending(&self, name: &str, limit: u64) -> AppResult<Vec<EventRecord>> {
        Ok(event::list_next_records(&self.db, name, limit)
            .await?
            .into_iter()
            .map(|record| EventRecord {
                id: record.id,
                payload: record.payload,
            })
            .collect())
    }

    pub async fn ack(&self, id: i64) -> AppResult<()> {
        event::mark_as_acknowledged(&self.db, id).await?;
        Ok(())
    }
}
