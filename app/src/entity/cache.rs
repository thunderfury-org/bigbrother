use chrono::Utc;
use sea_orm::{
    ActiveValue::Set,
    prelude::{DateTimeUtc, *},
    sea_query::OnConflict,
};

use super::model::cache;

pub async fn get_by_key<C>(db: &C, key: &str) -> Result<Option<cache::Model>, DbErr>
where
    C: ConnectionTrait,
{
    cache::Entity::find().filter(cache::Column::Key.eq(key)).one(db).await
}

pub async fn set_record<C>(db: &C, key: &str, value: &str, expired_at: Option<DateTimeUtc>) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let now = Utc::now();
    let record = cache::ActiveModel {
        key: Set(key.to_owned()),
        value: Set(value.to_owned()),
        expired_at: Set(expired_at),
        create_time: Set(now),
        update_time: Set(now),
        ..Default::default()
    };

    cache::Entity::insert(record)
        .on_conflict(
            OnConflict::column(cache::Column::Key)
                .update_columns([
                    cache::Column::Value,
                    cache::Column::ExpiredAt,
                    cache::Column::UpdateTime,
                ])
                .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}

pub async fn delete_by_key<C>(db: &C, key: &str) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    cache::Entity::delete_many()
        .filter(cache::Column::Key.eq(key))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn delete_expired<C>(db: &C) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    let result = cache::Entity::delete_many()
        .filter(cache::Column::ExpiredAt.is_not_null())
        .filter(cache::Column::ExpiredAt.lte(Utc::now()))
        .exec(db)
        .await?;

    Ok(result.rows_affected)
}

pub async fn exists<C>(db: &C, key: &str) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    cache::Entity::find()
        .filter(cache::Column::Key.eq(key))
        .count(db)
        .await
        .map(|count| count > 0)
}
