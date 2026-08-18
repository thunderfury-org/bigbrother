use chrono::Utc;
use sea_orm::{ActiveValue::Set, prelude::*};

use super::model::subscription;

pub async fn list_all<C>(db: &C) -> Result<Vec<subscription::Model>, DbErr>
where
    C: ConnectionTrait,
{
    subscription::Entity::find().all(db).await
}

pub async fn get_by_id<C>(db: &C, id: i64) -> Result<Option<subscription::Model>, DbErr>
where
    C: ConnectionTrait,
{
    subscription::Entity::find_by_id(id).one(db).await
}

pub async fn find_by_tmdb_id_and_media_type<C>(
    db: &C,
    tmdb_id: i32,
    media_type: &str,
) -> Result<Option<subscription::Model>, DbErr>
where
    C: ConnectionTrait,
{
    subscription::Entity::find()
        .filter(subscription::Column::TmdbId.eq(tmdb_id))
        .filter(subscription::Column::MediaType.eq(media_type))
        .one(db)
        .await
}

pub struct NewSubscription {
    pub tmdb_id: i32,
    pub media_type: String,
    pub title_zh: Option<String>,
    pub title_en: Option<String>,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
}

pub async fn insert_new<C>(db: &C, input: NewSubscription) -> Result<i64, DbErr>
where
    C: ConnectionTrait,
{
    let now = Utc::now();
    let active = subscription::ActiveModel {
        tmdb_id: Set(input.tmdb_id),
        media_type: Set(input.media_type),
        title_zh: Set(input.title_zh),
        title_en: Set(input.title_en),
        year: Set(input.year),
        poster_path: Set(input.poster_path),
        overview: Set(input.overview),
        create_time: Set(now),
        update_time: Set(now),
        ..Default::default()
    };

    let result = subscription::Entity::insert(active).exec(db).await?;
    Ok(result.last_insert_id)
}

pub async fn update_display<C>(
    db: &C,
    id: i64,
    year: Option<String>,
    poster_path: Option<String>,
    overview: Option<String>,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let active = subscription::ActiveModel {
        id: Set(id),
        year: Set(year),
        poster_path: Set(poster_path),
        overview: Set(overview),
        update_time: Set(Utc::now()),
        ..Default::default()
    };
    subscription::Entity::update(active).exec(db).await?;
    Ok(())
}

pub async fn delete_by_id<C>(db: &C, id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    subscription::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}
