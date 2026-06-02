use crate::application::file_index::FileIndexService;
use crate::application::file_index_import::ImportFileResult;
use crate::application::import::MetadataLookup;
use crate::application::import_ports::{MediaIdentifier, MediaImporter};
use crate::application::ports::{
    FileIndexRepository, ImportRecordRepository, SubscriptionRepository,
};
use crate::application::recorded_import::RecordedImportService;
use crate::domain::import_record::{ImportSource, ImportSourceKind};
use crate::error::AppResult;

use super::import_filter::filter_by_subscription;

pub(crate) async fn rescan_subscription<R, FI, I, D, RecordRepo>(
    subscription_id: i64,
    sub_repo: &R,
    file_index: &FileIndexService<FI>,
    identifier: &mut D,
    importer: &mut I,
    recorded: &RecordedImportService<RecordRepo>,
) -> AppResult<Vec<ImportFileResult>>
where
    R: SubscriptionRepository,
    FI: FileIndexRepository,
    I: MediaImporter,
    D: MediaIdentifier,
    RecordRepo: ImportRecordRepository,
{
    let subscription = sub_repo.get_by_id(subscription_id).await?.ok_or_else(|| {
        crate::error::AppError::NotFound(format!("subscription {subscription_id} not found"))
    })?;

    let query = subscription
        .title_en
        .as_deref()
        .or(subscription.title_zh.as_deref())
        .unwrap_or_default();

    if query.is_empty() {
        return Ok(Vec::new());
    }

    let search_results = file_index.search_files(query, 100).await?;
    if search_results.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for record in search_results {
        let file_id = record.id;
        let locations = record.locations;
        for location in locations {
            let source = ImportSource {
                kind: ImportSourceKind::FileIndex,
                raw: format!("file_index:{}:{}", file_id, location.file_name),
            };
            let hash = match record.hash_type.as_str() {
                "sha1" => crate::domain::share::FileHash::Sha1(record.hash_value.clone()),
                _ => crate::domain::share::FileHash::Md5(record.hash_value.clone()),
            };
            let raw_file = crate::domain::share::RawFile {
                id: Some(file_id),
                name: location.file_name,
                hash,
                size: record.size,
                path: location.file_path,
            };
            let descriptions = location.descriptions;
            let raw_files = vec![raw_file];

            let outcome = recorded
                .execute(source, || async {
                    let mut metadata_lookup = MetadataLookup::default();
                    let media_files =
                        metadata_lookup.build_media_files(raw_files.clone(), descriptions);
                    let identified = identifier.identify(media_files).await?;
                    let filtered_groups = filter_by_subscription(sub_repo, identified.groups).await;
                    importer
                        .import_groups(filtered_groups, identified.unmatched)
                        .await
                })
                .await;

            match outcome {
                Ok(imported) => {
                    for item in &imported {
                        results.push(ImportFileResult::from_imported(file_id, item));
                    }
                    if imported.is_empty() {
                        results.push(ImportFileResult::skipped(file_id, "no media matched"));
                    }
                }
                Err(err) => {
                    tracing::warn!(file_id, error = %err, "rescan import failed");
                    results.push(ImportFileResult::failed(file_id, err.to_string()));
                }
            }
        }
    }

    Ok(results)
}
