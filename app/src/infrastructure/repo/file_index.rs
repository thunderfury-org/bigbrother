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

impl FileIndexRepository for SeaOrmFileIndexRepository {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<()> {
        entity::file_index::record_files(&self.db, files).await
    }

    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>> {
        entity::file_index::search_files(&self.db, keyword, limit).await
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectOptions, Database};

    use super::*;
    use migration::{Migrator, MigratorTrait};

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
}
