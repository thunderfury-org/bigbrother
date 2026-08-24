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

    #[tokio::test]
    async fn search_files_omits_unrelated_location_of_same_fingerprint() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: None,
            },
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "other.mkv".into(),
                file_path: "/Other".into(),
                description: None,
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "movie.mkv");
    }

    #[tokio::test]
    async fn search_files_shared_description_fills_remaining_limit() {
        let repo = repo().await;
        let files = (0..5)
            .map(|index| FileIndexRecordInput {
                size: 100 + index,
                hash_type: "md5".into(),
                hash_value: format!("{index:032x}"),
                file_name: format!("episode-{index}.mkv"),
                file_path: "/Shows".into(),
                description: Some("shared keyword".into()),
            })
            .collect::<Vec<_>>();
        repo.record_files(&files).await.unwrap();

        let results = repo.search_files("shared", 3).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(
            results
                .iter()
                .all(|record| record.locations.iter().any(|location| {
                    location
                        .descriptions
                        .iter()
                        .any(|desc| desc == "shared keyword")
                }))
        );
    }

    #[tokio::test]
    async fn get_records_by_ids_hydrates_all_locations_and_descriptions() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie-a.mkv".into(),
                file_path: "/A".into(),
                description: Some("alpha".into()),
            },
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "movie-b.mkv".into(),
                file_path: "/B".into(),
                description: Some("beta".into()),
            },
            FileIndexRecordInput {
                size: 200,
                hash_type: "sha1".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "episode.mkv".into(),
                file_path: "/Shows".into(),
                description: Some("gamma".into()),
            },
        ])
        .await
        .unwrap();

        let all = repo.search_files("mkv", 20).await.unwrap();
        let ids = all.iter().map(|record| record.id).collect::<Vec<_>>();
        let results = repo.get_records_by_ids(&ids).await.unwrap();
        assert_eq!(results.len(), 2);

        let movie = results
            .iter()
            .find(|record| record.hash_type == "md5")
            .unwrap();
        assert_eq!(movie.locations.len(), 2);
        let mut movie_names = movie
            .locations
            .iter()
            .map(|location| location.file_name.as_str())
            .collect::<Vec<_>>();
        movie_names.sort();
        assert_eq!(movie_names, vec!["movie-a.mkv", "movie-b.mkv"]);
        let mut movie_descriptions = movie
            .locations
            .iter()
            .flat_map(|location| location.descriptions.clone())
            .collect::<Vec<_>>();
        movie_descriptions.sort();
        assert_eq!(movie_descriptions, vec!["alpha", "beta"]);

        let episode = results
            .iter()
            .find(|record| record.hash_type == "sha1")
            .unwrap();
        assert_eq!(episode.locations.len(), 1);
        assert_eq!(episode.locations[0].descriptions, vec!["gamma"]);
    }

    #[tokio::test]
    async fn search_files_ands_whitespace_tokens_across_dotted_filename() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "Love.Is.Blind.2020.S09E11.mkv".into(),
                file_path: "/Reality".into(),
                description: None,
            },
            FileIndexRecordInput {
                size: 200,
                hash_type: "md5".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "Half.Lives.2024.mkv".into(),
                file_path: "/Movies".into(),
                description: None,
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("Love Is Blind", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].locations[0].file_name,
            "Love.Is.Blind.2020.S09E11.mkv"
        );
    }

    #[tokio::test]
    async fn search_files_requires_every_token() {
        let repo = repo().await;
        repo.record_files(&[FileIndexRecordInput {
            size: 100,
            hash_type: "md5".into(),
            hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            file_name: "movie.2024.mkv".into(),
            file_path: "/Movies".into(),
            description: None,
        }])
        .await
        .unwrap();

        let hits = repo.search_files("movie 2024", 20).await.unwrap();
        assert_eq!(hits.len(), 1);

        let misses = repo.search_files("movie 2025", 20).await.unwrap();
        assert!(misses.is_empty());
    }

    #[tokio::test]
    async fn search_files_ands_tokens_across_name_and_description() {
        let repo = repo().await;
        repo.record_files(&[FileIndexRecordInput {
            size: 100,
            hash_type: "md5".into(),
            hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            file_name: "episode.mkv".into(),
            file_path: "/Shows".into(),
            description: Some("rare keyword".into()),
        }])
        .await
        .unwrap();

        let results = repo.search_files("episode rare", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "episode.mkv");
    }

    #[tokio::test]
    async fn search_files_ranks_filename_hits_before_path_and_description() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "unrelated.mkv".into(),
                file_path: "/Other".into(),
                description: Some("Love Is Blind".into()),
            },
            FileIndexRecordInput {
                size: 200,
                hash_type: "md5".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "clip.mkv".into(),
                file_path: "/Love/Is/Blind".into(),
                description: None,
            },
            FileIndexRecordInput {
                size: 300,
                hash_type: "md5".into(),
                hash_value: "cccccccccccccccccccccccccccccccc".into(),
                file_name: "Love.Is.Blind.S09E11.mkv".into(),
                file_path: "/Reality".into(),
                description: Some("other notes".into()),
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("Love Is Blind", 20).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(
            results[0].locations[0].file_name,
            "Love.Is.Blind.S09E11.mkv"
        );

        let limited = repo.search_files("Love Is Blind", 1).await.unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(
            limited[0].locations[0].file_name,
            "Love.Is.Blind.S09E11.mkv"
        );
    }

    #[tokio::test]
    async fn search_files_matches_consecutive_chinese_phrase_only() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                hash_type: "md5".into(),
                hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                file_name: "三体.S01E01.mkv".into(),
                file_path: "/Shows".into(),
                description: None,
            },
            FileIndexRecordInput {
                size: 200,
                hash_type: "md5".into(),
                hash_value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                file_name: "三.mkv".into(),
                file_path: "/Other".into(),
                description: Some("体".into()),
            },
            FileIndexRecordInput {
                size: 300,
                hash_type: "md5".into(),
                hash_value: "cccccccccccccccccccccccccccccccc".into(),
                file_name: "三.体.mkv".into(),
                file_path: "/Other".into(),
                description: None,
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("三体", 20).await.unwrap();
        let names = results
            .iter()
            .flat_map(|record| {
                record
                    .locations
                    .iter()
                    .map(|location| location.file_name.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 1);
        assert_eq!(names, vec!["三体.S01E01.mkv"]);
    }

    #[tokio::test]
    async fn search_files_updates_index_when_description_is_added_later() {
        let repo = repo().await;
        let file = FileIndexRecordInput {
            size: 100,
            hash_type: "md5".into(),
            hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            file_name: "movie.mkv".into(),
            file_path: "/Movies".into(),
            description: None,
        };
        repo.record_files(&[file.clone()]).await.unwrap();
        assert!(repo.search_files("rare", 20).await.unwrap().is_empty());

        let mut with_description = file;
        with_description.description = Some("rare keyword".into());
        repo.record_files(&[with_description]).await.unwrap();

        let results = repo.search_files("rare", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "movie.mkv");
    }

    #[tokio::test]
    async fn search_files_backfills_existing_locations() {
        use chrono::Utc;
        use sea_orm::{ActiveModelTrait, ActiveValue};

        use crate::{
            application::file_index::location_hash,
            infrastructure::entity::{file_index::backfill_file_location_fts, model},
        };

        let repo = repo().await;
        let now = Utc::now();
        let index = model::file_index::ActiveModel {
            size: ActiveValue::Set(100),
            hash_type: ActiveValue::Set("md5".into()),
            hash_value: ActiveValue::Set("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            create_time: ActiveValue::Set(now),
            update_time: ActiveValue::Set(now),
            ..Default::default()
        }
        .insert(&repo.db)
        .await
        .unwrap();
        model::file_location::ActiveModel {
            file_index_id: ActiveValue::Set(index.id),
            file_name: ActiveValue::Set("legacy.mkv".into()),
            file_path: ActiveValue::Set("/Archive".into()),
            location_hash: ActiveValue::Set(location_hash("/Archive", "legacy.mkv")),
            create_time: ActiveValue::Set(now),
            update_time: ActiveValue::Set(now),
            ..Default::default()
        }
        .insert(&repo.db)
        .await
        .unwrap();

        assert!(repo.search_files("legacy", 20).await.unwrap().is_empty());

        backfill_file_location_fts(&repo.db).await.unwrap();

        let results = repo.search_files("legacy", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "legacy.mkv");
    }

    #[tokio::test]
    async fn search_files_backfills_more_locations_than_sqlite_variable_limit() {
        use chrono::Utc;
        use sea_orm::{ActiveModelTrait, ActiveValue, ConnectionTrait};

        use crate::{
            application::file_index::location_hash,
            infrastructure::entity::{file_index::backfill_file_location_fts, model},
        };

        let repo = repo().await;
        let now = Utc::now();
        let index = model::file_index::ActiveModel {
            size: ActiveValue::Set(100),
            hash_type: ActiveValue::Set("md5".into()),
            hash_value: ActiveValue::Set("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            create_time: ActiveValue::Set(now),
            update_time: ActiveValue::Set(now),
            ..Default::default()
        }
        .insert(&repo.db)
        .await
        .unwrap();

        const COUNT: usize = 33_000;
        let mut sql = String::from(
            "INSERT INTO file_location (file_index_id, file_name, file_path, location_hash, create_time, update_time) VALUES ",
        );
        for i in 0..COUNT {
            if i > 0 {
                sql.push(',');
            }
            let name = format!("legacy-{i}.mkv");
            let path = "/Archive";
            let hash = location_hash(path, &name);
            sql.push_str(&format!(
                "({index_id}, '{name}', '{path}', '{hash}', datetime('now'), datetime('now'))",
                index_id = index.id,
            ));
        }
        repo.db.execute_unprepared(&sql).await.unwrap();

        backfill_file_location_fts(&repo.db).await.unwrap();

        let results = repo.search_files("legacy-32999", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "legacy-32999.mkv");
    }

    #[tokio::test]
    async fn file_index_search_does_not_keep_fts_content_or_like_indexes() {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};

        let repo = repo().await;
        repo.record_files(&[FileIndexRecordInput {
            size: 100,
            hash_type: "md5".into(),
            hash_value: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            file_name: "movie.mkv".into(),
            file_path: "/Movies".into(),
            description: Some("desc".into()),
        }])
        .await
        .unwrap();

        let names = repo
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                String::from(
                    "SELECT name FROM sqlite_master WHERE name IN (
                        'file_location_fts_content',
                        'idx-file-location-name',
                        'idx-file-location-path'
                    )",
                ),
            ))
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.try_get::<String>("", "name").unwrap())
            .collect::<Vec<_>>();
        assert!(names.is_empty(), "{names:?}");

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].locations[0].file_name, "movie.mkv");
        assert_eq!(results[0].locations[0].descriptions, vec!["desc"]);
    }
}
