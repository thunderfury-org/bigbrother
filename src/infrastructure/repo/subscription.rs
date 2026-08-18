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

#[async_trait::async_trait]
impl SubscriptionRepository for SeaOrmSubscriptionRepository {
    async fn list_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
        Ok(entity::subscription::list_all(&self.db)
            .await?
            .into_iter()
            .filter_map(to_record)
            .collect())
    }

    async fn get_by_id(&self, id: i64) -> AppResult<Option<SubscriptionRecord>> {
        Ok(entity::subscription::get_by_id(&self.db, id)
            .await?
            .and_then(to_record))
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
        .and_then(to_record))
    }

    async fn create(&self, input: &SubscriptionCreateInput) -> AppResult<i64> {
        let tmdb_id = i32::try_from(input.tmdb_id).map_err(|_| {
            AppError::InvalidParameter(format!("tmdb_id {} out of range", input.tmdb_id))
        })?;
        let id = entity::subscription::insert_new(
            &self.db,
            entity::subscription::NewSubscription {
                tmdb_id,
                media_type: input.media_type.as_str().to_owned(),
                title_zh: input.title_zh.clone(),
                title_en: input.title_en.clone(),
                year: input.year.clone(),
                poster_path: input.poster_path.clone(),
                overview: input.overview.clone(),
            },
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

    async fn update_display(
        &self,
        id: i64,
        year: Option<String>,
        poster_path: Option<String>,
        overview: Option<String>,
    ) -> AppResult<()> {
        entity::subscription::update_display(&self.db, id, year, poster_path, overview)
            .await
            .map_err(|e| AppError::Database(e.to_string(), false))?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> AppResult<()> {
        entity::subscription::delete_by_id(&self.db, id).await?;
        Ok(())
    }
}

fn to_record(model: entity::model::subscription::Model) -> Option<SubscriptionRecord> {
    Some(SubscriptionRecord {
        id: model.id,
        tmdb_id: u32::try_from(model.tmdb_id).ok()?,
        media_type: SubscriptionMediaType::from_str(&model.media_type)?,
        title_zh: model.title_zh,
        title_en: model.title_en,
        year: model.year,
        poster_path: model.poster_path,
        overview: model.overview,
        create_time: model.create_time,
        update_time: model.update_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::subscription::SubscriptionMediaType;
    use crate::migration::{Migrator, MigratorTrait};
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
            year: None,
            poster_path: None,
            overview: None,
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
            year: Some("2010".into()),
            poster_path: Some("/inception.jpg".into()),
            overview: Some("A thief who steals corporate secrets.".into()),
        };
        let id = repo.create(&input).await.unwrap();
        assert!(id > 0);

        let all = repo.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].tmdb_id, 27205);
        assert_eq!(all[0].year.as_deref(), Some("2010"));
        assert_eq!(all[0].poster_path.as_deref(), Some("/inception.jpg"));
        assert_eq!(
            all[0].overview.as_deref(),
            Some("A thief who steals corporate secrets.")
        );

        repo.update_display(
            id,
            Some("2011".into()),
            Some("/updated.jpg".into()),
            Some("Updated overview.".into()),
        )
        .await
        .unwrap();
        let updated = repo.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(updated.year.as_deref(), Some("2011"));
        assert_eq!(updated.poster_path.as_deref(), Some("/updated.jpg"));
        assert_eq!(updated.overview.as_deref(), Some("Updated overview."));

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
