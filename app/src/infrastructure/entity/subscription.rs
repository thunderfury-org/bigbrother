#![allow(dead_code)]

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

pub async fn insert_new<C>(
    db: &C,
    tmdb_id: i32,
    media_type: &str,
    title_zh: Option<String>,
    title_en: Option<String>,
) -> Result<i64, DbErr>
where
    C: ConnectionTrait,
{
    let now = Utc::now();
    let active = subscription::ActiveModel {
        tmdb_id: Set(tmdb_id),
        media_type: Set(media_type.to_owned()),
        title_zh: Set(title_zh),
        title_en: Set(title_en),
        create_time: Set(now),
        update_time: Set(now),
        ..Default::default()
    };

    let result = subscription::Entity::insert(active).exec(db).await?;
    Ok(result.last_insert_id)
}

pub async fn delete_by_id<C>(db: &C, id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    subscription::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}
