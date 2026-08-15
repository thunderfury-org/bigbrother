use chrono::Utc;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::{
    application::ports::{
        ImportRecordCreate, ImportRecordFilter, ImportRecordFinalize, ImportRecordPage,
        ImportRecordPaging, ImportRecordView,
    },
    domain::import_record::{ImportSourceKind, ImportStatus},
};

use super::model::import_record;

pub async fn insert<C>(db: &C, input: &ImportRecordCreate) -> Result<i64, DbErr>
where
    C: ConnectionTrait,
{
    let now = input.created_at;
    let model = import_record::ActiveModel {
        source_kind: Set(input.source_kind.as_str().to_owned()),
        source: Set(input.source.clone()),
        status: Set(ImportStatus::Running.as_str().to_owned()),
        summary_json: Set(None),
        error_kind: Set(None),
        error_message: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        finished_at: Set(None),
        ..Default::default()
    };

    let res = import_record::Entity::insert(model).exec(db).await?;
    Ok(res.last_insert_id)
}

pub async fn finalize<C>(db: &C, id: i64, update: &ImportRecordFinalize) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let now = Utc::now();
    import_record::Entity::update_many()
        .col_expr(import_record::Column::Status, update.status.as_str().into())
        .col_expr(
            import_record::Column::SummaryJson,
            Some(update.summary_json.clone()).into(),
        )
        .col_expr(
            import_record::Column::ErrorKind,
            update.error_kind.clone().into(),
        )
        .col_expr(
            import_record::Column::ErrorMessage,
            update.error_message.clone().into(),
        )
        .col_expr(
            import_record::Column::FinishedAt,
            Some(update.finished_at).into(),
        )
        .col_expr(import_record::Column::UpdatedAt, now.into())
        .filter(import_record::Column::Id.eq(id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn find_by_id<C>(db: &C, id: i64) -> Result<Option<ImportRecordView>, DbErr>
where
    C: ConnectionTrait,
{
    Ok(import_record::Entity::find_by_id(id)
        .one(db)
        .await?
        .map(model_to_view))
}

pub async fn list<C>(
    db: &C,
    filter: &ImportRecordFilter,
    paging: ImportRecordPaging,
) -> Result<ImportRecordPage, DbErr>
where
    C: ConnectionTrait,
{
    let mut query = import_record::Entity::find();

    if let Some(status) = filter.status {
        query = query.filter(import_record::Column::Status.eq(status.as_str()));
    }
    if let Some(source_kind) = filter.source_kind {
        query = query.filter(import_record::Column::SourceKind.eq(source_kind.as_str()));
    }
    if let Some(since) = filter.since {
        query = query.filter(import_record::Column::CreatedAt.gte(since));
    }
    if let Some(until) = filter.until {
        query = query.filter(import_record::Column::CreatedAt.lt(until));
    }
    if let Some(cursor) = paging.cursor {
        query = query.filter(import_record::Column::Id.lt(cursor));
    }

    let limit = paging.limit.max(1);
    let rows = query
        .order_by_desc(import_record::Column::Id)
        .limit(limit + 1)
        .all(db)
        .await?;

    let mut items: Vec<ImportRecordView> = rows.into_iter().map(model_to_view).collect();
    let next_cursor = if items.len() as u64 > limit {
        items.pop();
        items.last().map(|item| item.id)
    } else {
        None
    };

    Ok(ImportRecordPage { items, next_cursor })
}

fn model_to_view(model: import_record::Model) -> ImportRecordView {
    let status = ImportStatus::from_str(&model.status).unwrap_or(ImportStatus::Running);
    let source_kind = ImportSourceKind::from_str(&model.source_kind);
    ImportRecordView {
        id: model.id,
        source_kind,
        source: model.source,
        status,
        summary_json: model.summary_json,
        error_kind: model.error_kind,
        error_message: model.error_message,
        created_at: model.created_at,
        updated_at: model.updated_at,
        finished_at: model.finished_at,
    }
}
