use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, TransactionSession, TransactionTrait,
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
        let mut descriptions_by_location =
            load_descriptions_by_location_ids(&txn, &[file_location_id]).await?;
        let descriptions = descriptions_by_location
            .remove(&file_location_id)
            .unwrap_or_default();
        super::file_location_fts::upsert_location_fts(
            &txn,
            file_location_id,
            &file.file_name,
            &file.file_path,
            &descriptions,
        )
        .await?;
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

    let ranked_locations =
        super::file_location_fts::search_location_ids(db, keyword.trim(), limit).await?;
    if ranked_locations.is_empty() {
        return Ok(Vec::new());
    }

    let location_ids = ranked_locations
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let rank_by_location = ranked_locations.into_iter().collect::<HashMap<_, _>>();
    let locations = load_locations_in_order(db, &location_ids).await?;
    let hydration = LocationHydration::load(db, &locations).await?;
    let mut by_fingerprint = HashMap::new();
    let mut order = Vec::new();
    for location in &locations {
        let rank = rank_by_location.get(&location.id).copied().unwrap_or(2);
        if !by_fingerprint.contains_key(&location.file_index_id) {
            if order.len() >= limit as usize {
                continue;
            }
            order.push(location.file_index_id);
        }
        add_location_match(&mut by_fingerprint, location, &hydration, rank);
    }

    Ok(order
        .into_iter()
        .filter_map(|id| by_fingerprint.remove(&id))
        .collect())
}

pub async fn backfill_file_location_fts<C>(db: &C) -> AppResult<()>
where
    C: ConnectionTrait + TransactionTrait,
{
    const BATCH_SIZE: u64 = 500;
    let txn = db.begin().await?;
    loop {
        let missing_ids = super::file_location_fts::missing_location_ids(&txn, BATCH_SIZE).await?;
        if missing_ids.is_empty() {
            break;
        }

        let locations = load_locations_in_order(&txn, &missing_ids).await?;
        let descriptions = load_descriptions_by_location_ids(&txn, &missing_ids).await?;
        for location in locations {
            let descs = descriptions.get(&location.id).cloned().unwrap_or_default();
            super::file_location_fts::upsert_location_fts(
                &txn,
                location.id,
                &location.file_name,
                &location.file_path,
                &descs,
            )
            .await?;
        }
    }
    txn.commit().await?;
    Ok(())
}

pub async fn get_records_by_ids<C>(db: &C, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>>
where
    C: ConnectionTrait,
{
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let indices = file_index::Entity::find()
        .filter(file_index::Column::Id.is_in(ids.to_vec()))
        .all(db)
        .await?;
    if indices.is_empty() {
        return Ok(Vec::new());
    }

    let index_ids = indices.iter().map(|index| index.id).collect::<Vec<_>>();
    let locations = file_location::Entity::find()
        .filter(file_location::Column::FileIndexId.is_in(index_ids))
        .all(db)
        .await?;
    let location_ids = locations
        .iter()
        .map(|location| location.id)
        .collect::<Vec<_>>();
    let descriptions = load_descriptions_by_location_ids(db, &location_ids).await?;

    let mut locations_by_index: HashMap<i64, Vec<file_location::Model>> = HashMap::new();
    for location in locations {
        locations_by_index
            .entry(location.file_index_id)
            .or_default()
            .push(location);
    }

    let mut results = Vec::with_capacity(indices.len());
    for index in indices {
        let location_records = locations_by_index
            .remove(&index.id)
            .unwrap_or_default()
            .into_iter()
            .map(|location| FileLocationRecord {
                descriptions: descriptions.get(&location.id).cloned().unwrap_or_default(),
                file_name: location.file_name,
                file_path: location.file_path,
            })
            .collect();

        results.push(FileSearchRecord {
            id: index.id,
            size: index.size.try_into().unwrap_or_default(),
            hash_type: index.hash_type,
            hash_value: index.hash_value,
            locations: location_records,
            rank: 0,
        });
    }

    Ok(results)
}

fn add_location_match(
    by_fingerprint: &mut HashMap<i64, FileSearchRecord>,
    location: &file_location::Model,
    hydration: &LocationHydration,
    rank: i64,
) {
    let Some(index) = hydration.indexes.get(&location.file_index_id) else {
        return;
    };

    let descriptions = hydration
        .descriptions
        .get(&location.id)
        .cloned()
        .unwrap_or_default();
    let entry = by_fingerprint
        .entry(index.id)
        .or_insert_with(|| FileSearchRecord {
            id: index.id,
            size: index.size.try_into().unwrap_or_default(),
            hash_type: index.hash_type.clone(),
            hash_value: index.hash_value.clone(),
            locations: Vec::new(),
            rank,
        });
    if rank < entry.rank {
        entry.rank = rank;
    }

    if let Some(existing) = entry.locations.iter_mut().find(|existing| {
        existing.file_name == location.file_name && existing.file_path == location.file_path
    }) {
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

async fn load_locations_in_order<C>(db: &C, ids: &[i64]) -> Result<Vec<file_location::Model>, DbErr>
where
    C: ConnectionTrait,
{
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let locations = file_location::Entity::find()
        .filter(file_location::Column::Id.is_in(ids.to_vec()))
        .all(db)
        .await?;
    let mut by_id = HashMap::with_capacity(locations.len());
    for location in locations {
        by_id.insert(location.id, location);
    }
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

struct LocationHydration {
    indexes: HashMap<i64, file_index::Model>,
    descriptions: HashMap<i64, Vec<String>>,
}

impl LocationHydration {
    async fn load<C>(db: &C, locations: &[file_location::Model]) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if locations.is_empty() {
            return Ok(Self {
                indexes: HashMap::new(),
                descriptions: HashMap::new(),
            });
        }

        let index_ids = locations
            .iter()
            .map(|location| location.file_index_id)
            .collect::<Vec<_>>();
        let location_ids = locations
            .iter()
            .map(|location| location.id)
            .collect::<Vec<_>>();
        Ok(Self {
            indexes: load_indexes_by_ids(db, &index_ids).await?,
            descriptions: load_descriptions_by_location_ids(db, &location_ids).await?,
        })
    }
}

async fn load_indexes_by_ids<C>(
    db: &C,
    ids: &[i64],
) -> Result<HashMap<i64, file_index::Model>, DbErr>
where
    C: ConnectionTrait,
{
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = file_index::Entity::find()
        .filter(file_index::Column::Id.is_in(ids.to_vec()))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

async fn load_descriptions_by_location_ids<C>(
    db: &C,
    location_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, DbErr>
where
    C: ConnectionTrait,
{
    if location_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let links = file_location_description::Entity::find()
        .filter(file_location_description::Column::FileLocationId.is_in(location_ids.to_vec()))
        .order_by_asc(file_location_description::Column::Id)
        .all(db)
        .await?;
    if links.is_empty() {
        return Ok(HashMap::new());
    }

    let description_ids = links
        .iter()
        .map(|link| link.file_description_id)
        .collect::<Vec<_>>();
    let descriptions = file_description::Entity::find()
        .filter(file_description::Column::Id.is_in(description_ids))
        .all(db)
        .await?;
    let description_by_id = descriptions
        .into_iter()
        .map(|description| (description.id, description.description))
        .collect::<HashMap<_, _>>();

    let mut descriptions_by_location: HashMap<i64, Vec<String>> = HashMap::new();
    for link in links {
        let Some(description) = description_by_id.get(&link.file_description_id) else {
            continue;
        };
        let entry = descriptions_by_location
            .entry(link.file_location_id)
            .or_default();
        push_unique(entry, description.clone());
    }
    Ok(descriptions_by_location)
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
