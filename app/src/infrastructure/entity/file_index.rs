use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, TransactionSession, TransactionTrait,
};

use crate::{
    application::{
        file_index::{description_hash, location_hash},
        ports::{FileIndexRecordInput, FileLocationRecord, FileSearchRecord},
    },
    error::{AppError, AppResult},
    infrastructure::entity::model::{
        file_description, file_index, file_location, file_location_description,
    },
};

pub async fn record_files<C>(db: &C, files: &[FileIndexRecordInput]) -> AppResult<()>
where
    C: ConnectionTrait + TransactionTrait,
{
    let txn = db.begin().await?;
    for file in files {
        let file_index_id = find_or_insert_file_index(&txn, file).await?;
        let file_location_id = find_or_insert_file_location(&txn, file_index_id, file).await?;
        if let Some(description) = file.description.as_deref() {
            let file_description_id = find_or_insert_description(&txn, description).await?;
            link_description(&txn, file_location_id, file_description_id).await?;
        }
    }
    txn.commit().await?;
    Ok(())
}

pub async fn search_files<C>(db: &C, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>
where
    C: ConnectionTrait,
{
    if keyword.trim().is_empty() {
        return Ok(Vec::new());
    }

    let trimmed = keyword.trim();
    let mut by_fingerprint = BTreeMap::new();

    let locations = file_location::Entity::find()
        .filter(
            Condition::any()
                .add(file_location::Column::FileName.contains(trimmed))
                .add(file_location::Column::FilePath.contains(trimmed)),
        )
        .order_by_asc(file_location::Column::Id)
        .all(db)
        .await?;
    for location in locations {
        add_location_match(db, &mut by_fingerprint, &location).await?;
    }

    let pattern = format!("%{}%", trimmed);
    let description_matches = file_description::Entity::find()
        .filter(file_description::Column::Description.like(pattern))
        .limit(limit)
        .all(db)
        .await?;

    for description in description_matches {
        let links = file_location_description::Entity::find()
            .filter(file_location_description::Column::FileDescriptionId.eq(description.id))
            .all(db)
            .await?;

        for link in links {
            let Some(location) = file_location::Entity::find_by_id(link.file_location_id)
                .one(db)
                .await?
            else {
                continue;
            };
            add_location_match(db, &mut by_fingerprint, &location).await?;
        }
    }

    let mut results = by_fingerprint.into_values().collect::<Vec<_>>();
    results.truncate(limit as usize);
    Ok(results)
}

async fn add_location_match<C>(
    db: &C,
    by_fingerprint: &mut BTreeMap<i64, FileSearchRecord>,
    location: &file_location::Model,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let Some(index) = file_index::Entity::find_by_id(location.file_index_id).one(db).await? else {
        return Ok(());
    };

    let descriptions = descriptions_for_location(db, location.id).await?;
    let entry = by_fingerprint
        .entry(index.id)
        .or_insert_with(|| FileSearchRecord {
            size: index.size.try_into().unwrap_or_default(),
            hash_type: index.hash_type.clone(),
            hash_value: index.hash_value.clone(),
            locations: Vec::new(),
        });

    if let Some(existing) = entry
        .locations
        .iter_mut()
        .find(|existing| existing.file_name == location.file_name && existing.file_path == location.file_path)
    {
        for description in descriptions {
            push_unique(&mut existing.descriptions, description);
        }
    } else {
        entry.locations.push(FileLocationRecord {
            file_name: location.file_name.clone(),
            file_path: location.file_path.clone(),
            descriptions,
        });
    }

    Ok(())
}

async fn find_or_insert_file_index<C>(db: &C, file: &FileIndexRecordInput) -> AppResult<i64>
where
    C: ConnectionTrait,
{
    if let Some(existing) = file_index::Entity::find()
        .filter(file_index::Column::Size.eq(size_as_i64(file.size)?))
        .filter(file_index::Column::HashType.eq(&file.hash_type))
        .filter(file_index::Column::HashValue.eq(&file.hash_value))
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }

    let now = Utc::now();
    let inserted = file_index::ActiveModel {
        size: ActiveValue::Set(size_as_i64(file.size)?),
        hash_type: ActiveValue::Set(file.hash_type.clone()),
        hash_value: ActiveValue::Set(file.hash_value.clone()),
        create_time: ActiveValue::Set(now),
        update_time: ActiveValue::Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(inserted.id)
}

async fn find_or_insert_file_location<C>(
    db: &C,
    file_index_id: i64,
    file: &FileIndexRecordInput,
) -> AppResult<i64>
where
    C: ConnectionTrait,
{
    let hash = location_hash(&file.file_path, &file.file_name);
    if let Some(existing) = file_location::Entity::find()
        .filter(file_location::Column::FileIndexId.eq(file_index_id))
        .filter(file_location::Column::LocationHash.eq(&hash))
        .one(db)
        .await?
    {
        if existing.file_name != file.file_name || existing.file_path != file.file_path {
            return Err(AppError::Internal(
                "file location hash collision or normalization conflict".into(),
            ));
        }
        return Ok(existing.id);
    }

    let now = Utc::now();
    let inserted = file_location::ActiveModel {
        file_index_id: ActiveValue::Set(file_index_id),
        file_name: ActiveValue::Set(file.file_name.clone()),
        file_path: ActiveValue::Set(file.file_path.clone()),
        location_hash: ActiveValue::Set(hash),
        create_time: ActiveValue::Set(now),
        update_time: ActiveValue::Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(inserted.id)
}

async fn find_or_insert_description<C>(db: &C, description: &str) -> Result<i64, DbErr>
where
    C: ConnectionTrait,
{
    let hash = description_hash(description);
    if let Some(existing) = file_description::Entity::find()
        .filter(file_description::Column::ContentHash.eq(&hash))
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }

    let inserted = file_description::ActiveModel {
        content_hash: ActiveValue::Set(hash),
        description: ActiveValue::Set(description.trim().to_owned()),
        create_time: ActiveValue::Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(inserted.id)
}

async fn link_description<C>(
    db: &C,
    file_location_id: i64,
    file_description_id: i64,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let exists = file_location_description::Entity::find()
        .filter(file_location_description::Column::FileLocationId.eq(file_location_id))
        .filter(file_location_description::Column::FileDescriptionId.eq(file_description_id))
        .one(db)
        .await?
        .is_some();
    if exists {
        return Ok(());
    }

    file_location_description::ActiveModel {
        file_location_id: ActiveValue::Set(file_location_id),
        file_description_id: ActiveValue::Set(file_description_id),
        create_time: ActiveValue::Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn descriptions_for_location<C>(db: &C, location_id: i64) -> Result<Vec<String>, DbErr>
where
    C: ConnectionTrait,
{
    let links = file_location_description::Entity::find()
        .filter(file_location_description::Column::FileLocationId.eq(location_id))
        .all(db)
        .await?;

    let mut descriptions = Vec::new();
    for link in links {
        if let Some(description) = file_description::Entity::find_by_id(link.file_description_id)
            .one(db)
            .await?
        {
            push_unique(&mut descriptions, description.description);
        }
    }
    Ok(descriptions)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn size_as_i64(size: u64) -> AppResult<i64> {
    size.try_into()
        .map_err(|_| AppError::InvalidParameter("file size exceeds i64".into()))
}
