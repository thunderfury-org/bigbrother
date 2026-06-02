#![allow(dead_code)]

use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{SubscriptionCreateInput, SubscriptionRecord, SubscriptionRepository},
    domain::subscription::SubscriptionMediaType,
    error::AppResult,
    infrastructure::entity,
};

#[derive(Clone)]
pub struct SeaOrmSubscriptionRepository {
    db: DatabaseConnection,
}

impl SeaOrmSubscriptionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl SubscriptionRepository for SeaOrmSubscriptionRepository {
    async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
        Ok(entity::subscription::list_all(&self.db)
            .await?
            .into_iter()
            .map(to_record)
            .collect())
    }

    async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>> {
        Ok(entity::subscription::get_by_id(&self.db, id)
            .await?
            .map(to_record))
    }

    async fn find_by_tmdb_id(
        &self,
        tmdb_id: u32,
        media_type: &SubscriptionMediaType,
    ) -> AppResult<Option<SubscriptionRecord>> {
        Ok(entity::subscription::find_by_tmdb_id_and_media_type(
            &self.db,
            tmdb_id as i32,
            media_type.as_str(),
        )
        .await?
        .map(to_record))
    }

    async fn create(&self, input: &SubscriptionCreateInput) -> AppResult<i64> {
        let id = entity::subscription::insert_new(
            &self.db,
            input.tmdb_id as i32,
            input.media_type.as_str(),
            input.title_zh.clone(),
            input.title_en.clone(),
        )
        .await?;
        Ok(id)
    }

    async fn delete(&self, id: i64) -> AppResult<()> {
        entity::subscription::delete_by_id(&self.db, id).await?;
        Ok(())
    }
}

fn to_record(model: entity::model::subscription::Model) -> SubscriptionRecord {
    SubscriptionRecord {
        id: model.id,
        tmdb_id: model.tmdb_id as u32,
        media_type: SubscriptionMediaType::from_str(&model.media_type)
            .expect("invalid media_type in database"),
        title_zh: model.title_zh,
        title_en: model.title_en,
        create_time: model.create_time,
        update_time: model.update_time,
    }
}
