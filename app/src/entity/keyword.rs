use chrono::Utc;
use sea_orm::{ActiveValue::Set, prelude::*, sea_query::OnConflict};

use super::model::keyword;

pub async fn list_all_keywords<C>(db: &C) -> Result<Vec<keyword::Model>, DbErr>
where
    C: ConnectionTrait,
{
    keyword::Entity::find().all(db).await
}

pub async fn add_new_keyword<C>(db: &C, value: &str) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let f = keyword::ActiveModel {
        value: Set(value.to_owned()),
        create_time: Set(Utc::now()),
        ..Default::default()
    };

    keyword::Entity::insert(f)
        .on_conflict(OnConflict::new().do_nothing().to_owned())
        .exec(db)
        .await?;

    Ok(())
}

pub async fn get_keyword<C>(db: &C, value: &str) -> Result<Option<keyword::Model>, DbErr>
where
    C: ConnectionTrait,
{
    keyword::Entity::find()
        .filter(keyword::Column::Value.eq(value))
        .one(db)
        .await
}

pub async fn delete_keyword_by_id<C>(db: &C, id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    keyword::Entity::delete_by_id(id).exec(db).await?;

    Ok(())
}
