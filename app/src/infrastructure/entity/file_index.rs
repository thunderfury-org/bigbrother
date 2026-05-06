use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};

use crate::{
    application::{
        file_index::{description_hash, location_hash},
        ports::{FileIndexRecordInput, FileSearchRecord},
    },
    error::{AppError, AppResult},
    infrastructure::entity::model::{
        file_description, file_index, file_location, file_location_description,
    },
};

pub async fn record_files<C>(db: &C, files: &[FileIndexRecordInput]) -> AppResult<usize>
where
    C: ConnectionTrait,
{
    for file in files {
        let file_index_id = find_or_insert_file_index(db, file).await?;
        let file_location_id = find_or_insert_file_location(db, file_index_id, file).await?;
        if let Some(description) = file.description.as_deref() {
            let file_description_id = find_or_insert_description(db, description).await?;
            link_description(db, file_location_id, file_description_id).await?;
        }
    }
    Ok(files.len())
}

pub async fn search_files<C>(db: &C, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>
where
    C: ConnectionTrait,
{
    if keyword.trim().is_empty() {
        return Ok(Vec::new());
    }

    let locations = file_location::Entity::find()
        .filter(
            Condition::any()
                .add(file_location::Column::FileName.contains(keyword.trim()))
                .add(file_location::Column::FilePath.contains(keyword.trim())),
        )
        .order_by_asc(file_location::Column::Id)
        .limit(limit)
        .all(db)
        .await?;

    let mut by_location = BTreeMap::new();
    for location in locations {
        if let Some(record) = record_for_location(db, &location).await? {
            by_location.insert(location.id, record);
        }
    }

    let pattern = format!("%{}%", keyword.trim());
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
            if !by_location.contains_key(&location.id)
                && let Some(record) = record_for_location(db, &location).await?
            {
                by_location.insert(location.id, record);
            }
            if let Some(record) = by_location.get_mut(&location.id) {
                push_unique(&mut record.descriptions, description.description.clone());
            }
        }
    }

    let location_ids = by_location.keys().copied().collect::<Vec<_>>();
    for location_id in location_ids {
        let descriptions = descriptions_for_location(db, location_id).await?;
        if let Some(record) = by_location.get_mut(&location_id) {
            for description in descriptions {
                push_unique(&mut record.descriptions, description);
            }
        }
    }

    Ok(by_location.into_values().take(limit as usize).collect())
}

async fn record_for_location<C>(
    db: &C,
    location: &file_location::Model,
) -> Result<Option<FileSearchRecord>, DbErr>
where
    C: ConnectionTrait,
{
    let Some(index) = file_index::Entity::find_by_id(location.file_index_id)
        .one(db)
        .await?
    else {
        return Ok(None);
    };

    Ok(Some(FileSearchRecord {
        file_name: location.file_name.clone(),
        file_path: location.file_path.clone(),
        size: index.size.try_into().unwrap_or_default(),
        md5: index.md5,
        sha1: index.sha1,
        descriptions: Vec::new(),
    }))
}

async fn find_or_insert_file_index<C>(db: &C, file: &FileIndexRecordInput) -> AppResult<i64>
where
    C: ConnectionTrait,
{
    if let Some(md5) = file.md5.as_deref()
        && let Some(existing) = file_index::Entity::find()
            .filter(file_index::Column::Size.eq(size_as_i64(file.size)?))
            .filter(file_index::Column::Md5.eq(md5))
            .one(db)
            .await?
    {
        update_missing_hashes(db, &existing, file).await?;
        return Ok(existing.id);
    }

    if let Some(sha1) = file.sha1.as_deref()
        && let Some(existing) = file_index::Entity::find()
            .filter(file_index::Column::Size.eq(size_as_i64(file.size)?))
            .filter(file_index::Column::Sha1.eq(sha1))
            .one(db)
            .await?
    {
        update_missing_hashes(db, &existing, file).await?;
        return Ok(existing.id);
    }

    let now = Utc::now();
    let inserted = file_index::ActiveModel {
        size: ActiveValue::Set(size_as_i64(file.size)?),
        md5: ActiveValue::Set(file.md5.clone()),
        sha1: ActiveValue::Set(file.sha1.clone()),
        create_time: ActiveValue::Set(now),
        update_time: ActiveValue::Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(inserted.id)
}

async fn update_missing_hashes<C>(
    db: &C,
    existing: &file_index::Model,
    file: &FileIndexRecordInput,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let mut active: file_index::ActiveModel = existing.clone().into();
    let mut changed = false;
    if existing.md5.is_none() && file.md5.is_some() {
        active.md5 = ActiveValue::Set(file.md5.clone());
        changed = true;
    }
    if existing.sha1.is_none() && file.sha1.is_some() {
        active.sha1 = ActiveValue::Set(file.sha1.clone());
        changed = true;
    }
    if changed {
        active.update_time = ActiveValue::Set(Utc::now());
        active.update(db).await?;
    }
    Ok(())
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
            descriptions.push(description.description);
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
