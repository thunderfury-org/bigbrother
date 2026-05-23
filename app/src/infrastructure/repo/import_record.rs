use sea_orm::DatabaseConnection;

use crate::{
    application::ports::{
        ImportRecordCreate, ImportRecordFilter, ImportRecordFinalize, ImportRecordPage,
        ImportRecordPaging, ImportRecordRepository, ImportRecordView,
    },
    error::AppResult,
    infrastructure::entity,
};

#[derive(Clone)]
pub struct SeaOrmImportRecordRepository {
    db: DatabaseConnection,
}

impl SeaOrmImportRecordRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl ImportRecordRepository for SeaOrmImportRecordRepository {
    async fn create(&self, input: &ImportRecordCreate) -> AppResult<i64> {
        Ok(entity::import_record::insert(&self.db, input).await?)
    }

    async fn finalize(&self, id: i64, update: &ImportRecordFinalize) -> AppResult<()> {
        entity::import_record::finalize(&self.db, id, update).await?;
        Ok(())
    }

    async fn get(&self, id: i64) -> AppResult<Option<ImportRecordView>> {
        Ok(entity::import_record::find_by_id(&self.db, id).await?)
    }

    async fn list(
        &self,
        filter: &ImportRecordFilter,
        paging: ImportRecordPaging,
    ) -> AppResult<ImportRecordPage> {
        Ok(entity::import_record::list(&self.db, filter, paging).await?)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};

    use crate::domain::import_record::{ImportSourceKind, ImportStatus};

    use super::*;

    async fn repo() -> SeaOrmImportRecordRepository {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        SeaOrmImportRecordRepository::new(db)
    }

    fn quark_at(seconds: i64) -> ImportRecordCreate {
        ImportRecordCreate {
            source_kind: ImportSourceKind::Quark,
            source: format!("https://pan.quark.cn/s/{seconds}"),
            created_at: Utc.timestamp_opt(seconds, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn create_returns_id_and_get_finds_running_record() {
        let repo = repo().await;
        let input = quark_at(1_700_000_000);
        let id = repo.create(&input).await.unwrap();

        let view = repo.get(id).await.unwrap().unwrap();
        assert_eq!(view.id, id);
        assert_eq!(view.source_kind, ImportSourceKind::Quark);
        assert_eq!(view.source, input.source);
        assert_eq!(view.status, ImportStatus::Running);
        assert!(view.summary_json.is_none());
        assert!(view.error_kind.is_none());
        assert!(view.finished_at.is_none());
        assert_eq!(view.created_at, input.created_at);
    }

    #[tokio::test]
    async fn finalize_writes_terminal_status_and_summary_and_finished_at() {
        let repo = repo().await;
        let id = repo.create(&quark_at(1_700_000_000)).await.unwrap();

        let finished_at = Utc.timestamp_opt(1_700_000_500, 0).unwrap();
        repo.finalize(
            id,
            &ImportRecordFinalize {
                status: ImportStatus::Succeeded,
                summary_json: "{\"foo\":1}".into(),
                error_kind: None,
                error_message: None,
                finished_at,
            },
        )
        .await
        .unwrap();

        let view = repo.get(id).await.unwrap().unwrap();
        assert_eq!(view.status, ImportStatus::Succeeded);
        assert_eq!(view.summary_json.as_deref(), Some("{\"foo\":1}"));
        assert_eq!(view.finished_at, Some(finished_at));
    }

    #[tokio::test]
    async fn finalize_persists_failure_error_classification() {
        let repo = repo().await;
        let id = repo.create(&quark_at(1_700_000_000)).await.unwrap();

        repo.finalize(
            id,
            &ImportRecordFinalize {
                status: ImportStatus::Failed,
                summary_json: "{}".into(),
                error_kind: Some("Internal".into()),
                error_message: Some("upstream timeout".into()),
                finished_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let view = repo.get(id).await.unwrap().unwrap();
        assert_eq!(view.status, ImportStatus::Failed);
        assert_eq!(view.error_kind.as_deref(), Some("Internal"));
        assert_eq!(view.error_message.as_deref(), Some("upstream timeout"));
    }

    #[tokio::test]
    async fn list_returns_newest_first_by_default() {
        let repo = repo().await;
        for seconds in [1_700_000_000_i64, 1_700_000_100, 1_700_000_200] {
            repo.create(&quark_at(seconds)).await.unwrap();
        }

        let page = repo
            .list(
                &ImportRecordFilter::default(),
                ImportRecordPaging {
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .unwrap();

        let sources: Vec<_> = page.items.iter().map(|item| item.source.clone()).collect();
        assert_eq!(
            sources,
            vec![
                "https://pan.quark.cn/s/1700000200",
                "https://pan.quark.cn/s/1700000100",
                "https://pan.quark.cn/s/1700000000",
            ]
        );
        assert_eq!(page.next_cursor, None);
    }

    #[tokio::test]
    async fn list_filters_by_status() {
        let repo = repo().await;
        let succeeded_id = repo.create(&quark_at(1_700_000_000)).await.unwrap();
        let _running_id = repo.create(&quark_at(1_700_000_100)).await.unwrap();
        repo.finalize(
            succeeded_id,
            &ImportRecordFinalize {
                status: ImportStatus::Succeeded,
                summary_json: "{}".into(),
                error_kind: None,
                error_message: None,
                finished_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let page = repo
            .list(
                &ImportRecordFilter {
                    status: Some(ImportStatus::Succeeded),
                    ..Default::default()
                },
                ImportRecordPaging {
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].status, ImportStatus::Succeeded);
    }

    #[tokio::test]
    async fn list_filters_by_source_kind() {
        let repo = repo().await;
        repo.create(&quark_at(1_700_000_000)).await.unwrap();
        repo.create(&ImportRecordCreate {
            source_kind: ImportSourceKind::Pan123,
            source: "https://www.123pan.com/s/abc".into(),
            created_at: Utc.timestamp_opt(1_700_000_100, 0).unwrap(),
        })
        .await
        .unwrap();

        let page = repo
            .list(
                &ImportRecordFilter {
                    source_kind: Some(ImportSourceKind::Pan123),
                    ..Default::default()
                },
                ImportRecordPaging {
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].source_kind, ImportSourceKind::Pan123);
    }

    #[tokio::test]
    async fn list_filters_by_created_at_since_and_until() {
        let repo = repo().await;
        for seconds in [1_700_000_000_i64, 1_700_000_100, 1_700_000_200] {
            repo.create(&quark_at(seconds)).await.unwrap();
        }

        let page = repo
            .list(
                &ImportRecordFilter {
                    since: Some(Utc.timestamp_opt(1_700_000_050, 0).unwrap()),
                    until: Some(
                        Utc.timestamp_opt(1_700_000_150, 0).unwrap() + Duration::seconds(1),
                    ),
                    ..Default::default()
                },
                ImportRecordPaging {
                    cursor: None,
                    limit: 10,
                },
            )
            .await
            .unwrap();

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].created_at.timestamp(), 1_700_000_100);
    }

    #[tokio::test]
    async fn list_paginates_with_cursor() {
        let repo = repo().await;
        let mut ids = Vec::new();
        for seconds in 0..5_i64 {
            ids.push(
                repo.create(&quark_at(1_700_000_000 + seconds * 10))
                    .await
                    .unwrap(),
            );
        }

        let first = repo
            .list(
                &ImportRecordFilter::default(),
                ImportRecordPaging {
                    cursor: None,
                    limit: 2,
                },
            )
            .await
            .unwrap();
        assert_eq!(first.items.len(), 2);
        assert_eq!(first.items[0].id, ids[4]);
        assert_eq!(first.items[1].id, ids[3]);
        let cursor = first.next_cursor.expect("expected next cursor");
        assert_eq!(cursor, ids[3]);

        let second = repo
            .list(
                &ImportRecordFilter::default(),
                ImportRecordPaging {
                    cursor: Some(cursor),
                    limit: 2,
                },
            )
            .await
            .unwrap();
        let second_ids: Vec<_> = second.items.iter().map(|item| item.id).collect();
        assert_eq!(second_ids, vec![ids[2], ids[1]]);
        let cursor = second.next_cursor.expect("expected next cursor");

        let third = repo
            .list(
                &ImportRecordFilter::default(),
                ImportRecordPaging {
                    cursor: Some(cursor),
                    limit: 2,
                },
            )
            .await
            .unwrap();
        assert_eq!(third.items.len(), 1);
        assert_eq!(third.items[0].id, ids[0]);
        assert!(third.next_cursor.is_none());
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_id() {
        let repo = repo().await;
        assert!(repo.get(9999).await.unwrap().is_none());
    }
}
