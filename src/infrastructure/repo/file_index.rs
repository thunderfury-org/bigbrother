use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    error::AppResult,
    infrastructure::entity,
};

#[derive(Clone)]
pub struct SeaOrmFileIndexRepository {
    db: DatabaseConnection,
}

impl SeaOrmFileIndexRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl FileIndexRepository for SeaOrmFileIndexRepository {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
        entity::file_index::record_files(&self.db, files).await
    }

    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>> {
        entity::file_index::search_files(&self.db, keyword, limit).await
    }

    async fn get_records_by_ids(&self, ids: &[i64]) -> AppResult<Vec<FileSearchRecord>> {
        entity::file_index::get_records_by_ids(&self.db, ids).await
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectOptions, Database};

    use super::*;
    use crate::migration::{Migrator, MigratorTrait};

    async fn repo() -> SeaOrmFileIndexRepository {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        SeaOrmFileIndexRepository::new(db)
    }

    #[tokio::test]
    async fn record_files_deduplicates_file_location_and_description() {
        let repo = repo().await;
        let files = vec![
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: Some("same desc".into()),
            },
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: Some("same desc".into()),
            },
        ];

        repo.record_files(&files).await.unwrap();
        repo.record_files(&files).await.unwrap();

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations.len(), 1);
        assert_eq!(results[0].locations[0].descriptions, vec!["same desc"]);
    }

    #[tokio::test]
    async fn record_files_keeps_multiple_locations_for_same_fingerprint() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie-a.mkv".into(),
                file_path: "/A".into(),
                description: Some("desc".into()),
            },
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie-b.mkv".into(),
                file_path: "/B".into(),
                description: Some("desc".into()),
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations.len(), 2);
    }

    #[tokio::test]
    async fn search_files_matches_description() {
        let repo = repo().await;
        repo.record_files(&[FileIndexRecordInput {
            size: 200,
            hash_type: "sha1".into(),
            hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            file_name: "episode.mkv".into(),
            file_path: "/Shows".into(),
            description: Some("rare keyword".into()),
        }])
        .await
        .unwrap();

        let results = repo.search_files("rare", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hash_type, "sha1");
        assert_eq!(
            results[0].hash_value,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[tokio::test]
    async fn record_files_does_not_merge_different_hash_types() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie.mkv".into(),
                file_path: "/A".into(),
                description: None,
            },
            FileIndexRecordInput {
                size: 100,
                hash_type: "sha1".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "movie.mkv".into(),
                file_path: "/A".into(),
                description: None,
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_files_respects_limit_for_name_matches() {
        let repo = repo().await;
        let files = (0..3)
            .map(|index| FileIndexRecordInput {
                size: 100 + index,
                hash_type: "md5".into(),
                hash_value: format!("{index:032x}"),
                file_name: format!("movie-{index}.mkv"),
                file_path: "/Movies".into(),
                description: None,
            })
            .collect::<Vec<_>>();

        repo.record_files(&files).await.unwrap();

        let results = repo.search_files("movie", 2).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn get_records_by_ids_returns_matching_fingerprints() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: Some("desc1".into()),
            },
            FileIndexRecordInput {
                size: 200,
                hash_type: "sha1".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "episode.mkv".into(),
                file_path: "/Shows".into(),
                description: Some("desc2".into()),
            },
        ])
        .await
        .unwrap();

        let all = repo.search_files("mkv", 20).await.unwrap();
        assert_eq!(all.len(), 2);

        let first_id = all[0].id;
        let results = repo.get_records_by_ids(&[first_id]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, first_id);
        assert_eq!(results[0].hash_type, all[0].hash_type);
        assert!(!results[0].locations.is_empty());
    }

    #[tokio::test]
    async fn get_records_by_ids_returns_empty_for_missing_ids() {
        let repo = repo().await;
        let results = repo.get_records_by_ids(&[9999]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn get_records_by_ids_returns_empty_for_empty_input() {
        let repo = repo().await;
        let results = repo.get_records_by_ids(&[]).await.unwrap();
        assert!(results.is_empty());
    }
}
