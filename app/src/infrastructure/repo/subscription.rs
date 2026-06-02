use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{SubscriptionCreateInput, SubscriptionRecord, SubscriptionRepository},
    domain::subscription::SubscriptionMediaType,
    error::{AppError, AppResult},
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
        let tmdb_id = i32::try_from(tmdb_id)
            .map_err(|_| AppError::InvalidParameter(format!("tmdb_id {tmdb_id} out of range")))?;
        Ok(entity::subscription::find_by_tmdb_id_and_media_type(
            &self.db,
            tmdb_id,
            media_type.as_str(),
        )
        .await?
        .map(to_record))
    }

    async fn create(&self, input: &SubscriptionCreateInput) -> AppResult<i64> {
        let tmdb_id = i32::try_from(input.tmdb_id).map_err(|_| {
            AppError::InvalidParameter(format!("tmdb_id {} out of range", input.tmdb_id))
        })?;
        let id = entity::subscription::insert_new(
            &self.db,
            tmdb_id,
            input.media_type.as_str(),
            input.title_zh.clone(),
            input.title_en.clone(),
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                AppError::InvalidParameter(format!(
                    "subscription already exists for tmdb_id={} media_type={}",
                    input.tmdb_id,
                    input.media_type.as_str()
                ))
            } else {
                AppError::Database(e.to_string(), false)
            }
        })?;
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
        tmdb_id: u32::try_from(model.tmdb_id).unwrap_or(0),
        media_type: SubscriptionMediaType::from_str(&model.media_type)
            .expect("invalid media_type in database"),
        title_zh: model.title_zh,
        title_en: model.title_en,
        create_time: model.create_time,
        update_time: model.update_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::subscription::SubscriptionMediaType;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};

    async fn fresh_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    #[tokio::test]
    async fn create_duplicate_returns_invalid_parameter() {
        let repo = SeaOrmSubscriptionRepository::new(fresh_db().await);
        let input = SubscriptionCreateInput {
            tmdb_id: 27205,
            media_type: SubscriptionMediaType::Movie,
            title_zh: None,
            title_en: Some("Inception".into()),
        };
        repo.create(&input).await.unwrap();
        let err = repo.create(&input).await.unwrap_err();
        assert!(matches!(err, AppError::InvalidParameter(_)));
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn crud_roundtrip() {
        let repo = SeaOrmSubscriptionRepository::new(fresh_db().await);
        let input = SubscriptionCreateInput {
            tmdb_id: 27205,
            media_type: SubscriptionMediaType::Movie,
            title_zh: Some("盗梦空间".into()),
            title_en: Some("Inception".into()),
        };
        let id = repo.create(&input).await.unwrap();
        assert!(id > 0);

        let all = repo.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].tmdb_id, 27205);

        let found = repo
            .find_by_tmdb_id(27205, &SubscriptionMediaType::Movie)
            .await
            .unwrap();
        assert!(found.is_some());

        let not_found = repo
            .find_by_tmdb_id(99999, &SubscriptionMediaType::Movie)
            .await
            .unwrap();
        assert!(not_found.is_none());

        repo.delete(id).await.unwrap();
        assert!(repo.list_all().await.unwrap().is_empty());
    }
}
