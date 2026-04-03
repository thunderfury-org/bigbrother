use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{KeywordRecord, KeywordRepository},
    entity,
    error::AppResult,
};

#[derive(Clone)]
pub struct SeaOrmKeywordRepository {
    db: DatabaseConnection,
}

impl SeaOrmKeywordRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl KeywordRepository for SeaOrmKeywordRepository {
    async fn list_all_keywords(&self) -> AppResult<Vec<KeywordRecord>> {
        Ok(entity::keyword::list_all_keywords(&self.db)
            .await?
            .into_iter()
            .map(|model| KeywordRecord {
                id: model.id,
                value: model.value,
            })
            .collect())
    }

    async fn add_keyword(&self, value: &str) -> AppResult<()> {
        entity::keyword::add_new_keyword(&self.db, value).await?;
        Ok(())
    }

    async fn delete_keyword(&self, id: i64) -> AppResult<()> {
        entity::keyword::delete_keyword_by_id(&self.db, id).await?;
        Ok(())
    }
}
