use chrono::Utc;
use sea_orm::{ActiveValue::Set, ConnectionTrait, DbErr, EntityTrait, sea_query::OnConflict};

use crate::application::ports::TelegramExportStateRecord;

use super::model::telegram_export_state;

pub async fn list_all<C>(db: &C) -> Result<Vec<TelegramExportStateRecord>, DbErr>
where
    C: ConnectionTrait,
{
    let models = telegram_export_state::Entity::find().all(db).await?;
    Ok(models
        .into_iter()
        .map(|model| TelegramExportStateRecord {
            source_type: model.source_type,
            source_value: model.source_value,
            description: model.description,
            status: model.status,
            error: model.error,
            attempt_count: model.attempt_count,
            first_seen_at: model.first_seen_at,
            last_attempt_at: model.last_attempt_at,
        })
        .collect())
}

pub async fn upsert<C>(db: &C, record: &TelegramExportStateRecord) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let now = Utc::now();
    let model = telegram_export_state::ActiveModel {
        source_type: Set(record.source_type.clone()),
        source_value: Set(record.source_value.clone()),
        description: Set(record.description.clone()),
        status: Set(record.status.clone()),
        error: Set(record.error.clone()),
        attempt_count: Set(record.attempt_count),
        first_seen_at: Set(record.first_seen_at.clone()),
        last_attempt_at: Set(record.last_attempt_at.clone()),
        create_time: Set(now),
        update_time: Set(now),
        ..Default::default()
    };

    telegram_export_state::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                telegram_export_state::Column::SourceType,
                telegram_export_state::Column::SourceValue,
            ])
            .update_columns([
                telegram_export_state::Column::Description,
                telegram_export_state::Column::Status,
                telegram_export_state::Column::Error,
                telegram_export_state::Column::AttemptCount,
                telegram_export_state::Column::FirstSeenAt,
                telegram_export_state::Column::LastAttemptAt,
                telegram_export_state::Column::UpdateTime,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;

    Ok(())
}
