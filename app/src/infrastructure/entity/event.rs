use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QuerySelect,
};

use super::model::event;

pub async fn list_next_records<C>(
    db: &C,
    name: &str,
    limit: u64,
) -> Result<Vec<event::Model>, DbErr>
where
    C: ConnectionTrait,
{
    event::Entity::find()
        .filter(event::Column::Name.eq(name))
        .filter(event::Column::Ack.eq(false))
        .order_by_id_asc()
        .limit(limit)
        .all(db)
        .await
}

pub async fn mark_as_acknowledged<C>(db: &C, id: i64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    event::Entity::update_many()
        .col_expr(event::Column::Ack, true.into())
        .filter(event::Column::Id.eq(id))
        .exec(db)
        .await?;

    Ok(())
}

pub async fn add_record<C>(db: &C, name: &str, payload: &str) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let new_event = event::ActiveModel {
        name: sea_orm::ActiveValue::Set(name.to_owned()),
        payload: sea_orm::ActiveValue::Set(payload.to_owned()),
        ack: sea_orm::ActiveValue::Set(false),
        create_time: sea_orm::ActiveValue::Set(chrono::Utc::now()),
        update_time: sea_orm::ActiveValue::Set(chrono::Utc::now()),
        ..Default::default()
    };
    new_event.insert(db).await?;

    Ok(())
}
