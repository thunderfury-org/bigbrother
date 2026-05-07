# File Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record every hash-identifiable resource seen from Telegram or manual import into a searchable database index.

**Architecture:** Add a file-index application service backed by SeaORM repositories and four SQLite tables: file identity, file location, de-duplicated description, and location-description links. CLI import indexes synchronously, while Telegram publishes an event that a background worker resolves and indexes asynchronously before the existing import decision path continues.

**Tech Stack:** Rust 2024, Tokio, SeaORM, SeaORM migration, SQLite, Teloxide, existing event bus, existing pan123/pan189/pan115 import gateways.

---

## File Structure

- Modify `Cargo.toml`
  - Add workspace dependency for `sha2`.
- Modify `app/Cargo.toml`
  - Add `sha2 = { workspace = true }`.
- Create `migration/src/m20260506_000000_create_table_file_index.rs`
  - Defines `file_index`, `file_location`, `file_description`, and `file_location_description`.
- Modify `migration/src/lib.rs`
  - Register the new migration after cache.
- Create `app/src/infrastructure/entity/model/file_index.rs`
  - SeaORM model for `file_index`.
- Create `app/src/infrastructure/entity/model/file_location.rs`
  - SeaORM model for `file_location`.
- Create `app/src/infrastructure/entity/model/file_description.rs`
  - SeaORM model for `file_description`.
- Create `app/src/infrastructure/entity/model/file_location_description.rs`
  - SeaORM model for `file_location_description`.
- Modify `app/src/infrastructure/entity/model/mod.rs`
  - Export the four new models.
- Create `app/src/infrastructure/entity/file_index.rs`
  - Low-level SeaORM upsert/search helpers.
- Modify `app/src/infrastructure/entity/mod.rs`
  - Export `file_index`.
- Create `app/src/infrastructure/repo/file_index.rs`
  - Implements the application repository port.
- Modify `app/src/infrastructure/repo/mod.rs`
  - Export `file_index`.
- Create `app/src/application/file_index.rs`
  - File-index models, hash normalization, `FileIndexService`, `FileIndexIngestService`, and unit tests.
- Modify `app/src/application/ports.rs`
  - Add `FileIndexRepository`, `FileIndexRecordInput`, and search result structs.
- Modify `app/src/application/mod.rs`
  - Export `file_index`.
- Modify `app/src/application/import/share/providers.rs`
  - Expose raw share file collection for ingest reuse.
- Modify `app/src/application/import/json.rs`
  - Expose JSON/fslink normalization for ingest reuse.
- Modify `app/src/application/import_media.rs`
  - Keep import behavior unchanged while sharing parsing helpers.
- Modify `app/src/bootstrap/services.rs`
  - Add file index service builders and runtime type aliases.
- Modify `app/src/bootstrap/app.rs`
  - Carry ingest directory path.
- Modify `app/src/bootstrap/mod.rs`
  - Wire file-index service into CLI/runtime and subscribe the event worker.
- Modify `app/src/interface/cli/mod.rs`
  - Add `--description/-d` to `import-share-url` and add `search-files`.
- Modify `app/src/main.rs`
  - Invoke synchronous CLI indexing and search.
- Create `app/src/interface/telegram/file_index.rs`
  - Telegram source extraction and event payload definitions.
- Modify `app/src/interface/telegram/mod.rs`
  - Publish index events for authorized private messages and monitored channel messages.
- Modify `app/src/interface/telegram/msg.rs`
  - Reuse source extraction helpers where possible and preserve current import notifications.

## Task 1: Add Schema, Entities, and Dependency

**Files:**
- Modify: `Cargo.toml`
- Modify: `app/Cargo.toml`
- Create: `migration/src/m20260506_000000_create_table_file_index.rs`
- Modify: `migration/src/lib.rs`
- Create: `app/src/infrastructure/entity/model/file_index.rs`
- Create: `app/src/infrastructure/entity/model/file_location.rs`
- Create: `app/src/infrastructure/entity/model/file_description.rs`
- Create: `app/src/infrastructure/entity/model/file_location_description.rs`
- Modify: `app/src/infrastructure/entity/model/mod.rs`

- [ ] **Step 1: Add failing migration registration test by compiling migration**

Run:

```bash
cargo test -p migration
```

Expected before this task: PASS. This establishes the baseline before adding schema.

- [ ] **Step 2: Add `sha2` dependency**

Edit root `Cargo.toml` workspace dependencies and add:

```toml
sha2 = "0.10"
```

Edit `app/Cargo.toml` dependencies and add:

```toml
sha2 = { workspace = true }
```

- [ ] **Step 3: Create migration**

Create `migration/src/m20260506_000000_create_table_file_index.rs`:

```rust
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FileIndex::Table)
                    .if_not_exists()
                    .col(pk_auto(FileIndex::Id))
                    .col(big_unsigned(FileIndex::Size))
                    .col(string_null(FileIndex::Md5))
                    .col(string_null(FileIndex::Sha1))
                    .col(timestamp(FileIndex::CreateTime))
                    .col(timestamp(FileIndex::UpdateTime))
                    .check(Expr::cust("md5 IS NOT NULL OR sha1 IS NOT NULL"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .unique()
                    .name("idx-file-index-size-md5")
                    .table(FileIndex::Table)
                    .col(FileIndex::Size)
                    .col(FileIndex::Md5)
                    .condition(Expr::cust("md5 IS NOT NULL"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .unique()
                    .name("idx-file-index-size-sha1")
                    .table(FileIndex::Table)
                    .col(FileIndex::Size)
                    .col(FileIndex::Sha1)
                    .condition(Expr::cust("sha1 IS NOT NULL"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileLocation::Table)
                    .if_not_exists()
                    .col(pk_auto(FileLocation::Id))
                    .col(big_integer(FileLocation::FileIndexId))
                    .col(text(FileLocation::FileName))
                    .col(text(FileLocation::FilePath))
                    .col(string(FileLocation::LocationHash))
                    .col(timestamp(FileLocation::CreateTime))
                    .col(timestamp(FileLocation::UpdateTime))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-file-index")
                            .from(FileLocation::Table, FileLocation::FileIndexId)
                            .to(FileIndex::Table, FileIndex::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-location-file-hash")
                            .col(FileLocation::FileIndexId)
                            .col(FileLocation::LocationHash),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-file-location-name")
                    .table(FileLocation::Table)
                    .col(FileLocation::FileName)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-file-location-path")
                    .table(FileLocation::Table)
                    .col(FileLocation::FilePath)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileDescription::Table)
                    .if_not_exists()
                    .col(pk_auto(FileDescription::Id))
                    .col(string(FileDescription::ContentHash))
                    .col(text(FileDescription::Description))
                    .col(timestamp(FileDescription::CreateTime))
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-description-hash")
                            .col(FileDescription::ContentHash),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(FileLocationDescription::Table)
                    .if_not_exists()
                    .col(pk_auto(FileLocationDescription::Id))
                    .col(big_integer(FileLocationDescription::FileLocationId))
                    .col(big_integer(FileLocationDescription::FileDescriptionId))
                    .col(timestamp(FileLocationDescription::CreateTime))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-description-location")
                            .from(
                                FileLocationDescription::Table,
                                FileLocationDescription::FileLocationId,
                            )
                            .to(FileLocation::Table, FileLocation::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-file-location-description-description")
                            .from(
                                FileLocationDescription::Table,
                                FileLocationDescription::FileDescriptionId,
                            )
                            .to(FileDescription::Table, FileDescription::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .unique()
                            .name("idx-file-location-description-link")
                            .col(FileLocationDescription::FileLocationId)
                            .col(FileLocationDescription::FileDescriptionId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(FileLocationDescription::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileDescription::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileLocation::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(FileIndex::Table).if_exists().to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum FileIndex {
    Table,
    Id,
    Size,
    Md5,
    Sha1,
    CreateTime,
    UpdateTime,
}

#[derive(DeriveIden)]
enum FileLocation {
    Table,
    Id,
    FileIndexId,
    FileName,
    FilePath,
    LocationHash,
    CreateTime,
    UpdateTime,
}

#[derive(DeriveIden)]
enum FileDescription {
    Table,
    Id,
    ContentHash,
    Description,
    CreateTime,
}

#[derive(DeriveIden)]
enum FileLocationDescription {
    Table,
    Id,
    FileLocationId,
    FileDescriptionId,
    CreateTime,
}
```

- [ ] **Step 4: Register migration**

Edit `migration/src/lib.rs`:

```rust
mod m20260506_000000_create_table_file_index;
```

Append the migration in `MigratorTrait::migrations()`:

```rust
Box::new(m20260506_000000_create_table_file_index::Migration),
```

- [ ] **Step 5: Add SeaORM model modules**

Create `app/src/infrastructure/entity/model/file_index.rs`:

```rust
//! `SeaORM` Entity for file_index.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_index")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub size: u64,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
```

Create `app/src/infrastructure/entity/model/file_location.rs`:

```rust
//! `SeaORM` Entity for file_location.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_location")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub file_index_id: i64,
    #[sea_orm(column_type = "Text")]
    pub file_name: String,
    #[sea_orm(column_type = "Text")]
    pub file_path: String,
    pub location_hash: String,
    pub create_time: DateTimeUtc,
    pub update_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
```

Create `app/src/infrastructure/entity/model/file_description.rs`:

```rust
//! `SeaORM` Entity for file_description.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_description")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub content_hash: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub create_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
```

Create `app/src/infrastructure/entity/model/file_location_description.rs`:

```rust
//! `SeaORM` Entity for file_location_description.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "file_location_description")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub file_location_id: i64,
    pub file_description_id: i64,
    pub create_time: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 6: Export entity models**

Edit `app/src/infrastructure/entity/model/mod.rs`:

```rust
pub mod file_description;
pub mod file_index;
pub mod file_location;
pub mod file_location_description;
```

- [ ] **Step 7: Run migration crate tests**

Run:

```bash
cargo test -p migration
```

Expected: PASS.

- [ ] **Step 8: Run app compilation check**

Run:

```bash
cargo check -p bigbrother
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml app/Cargo.toml migration/src/m20260506_000000_create_table_file_index.rs migration/src/lib.rs app/src/infrastructure/entity/model/file_index.rs app/src/infrastructure/entity/model/file_location.rs app/src/infrastructure/entity/model/file_description.rs app/src/infrastructure/entity/model/file_location_description.rs app/src/infrastructure/entity/model/mod.rs
git commit -m "add file index schema"
```

## Task 2: Add Application Models, Hashing, and Repository Port

**Files:**
- Create: `app/src/application/file_index.rs`
- Modify: `app/src/application/ports.rs`
- Modify: `app/src/application/mod.rs`

- [ ] **Step 1: Export module and port types**

Edit `app/src/application/mod.rs`:

```rust
pub mod file_index;
```

Append to `app/src/application/ports.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndexRecordInput {
    pub size: u64,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub file_name: String,
    pub file_path: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchRecord {
    pub file_name: String,
    pub file_path: String,
    pub size: u64,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    pub descriptions: Vec<String>,
}

pub trait FileIndexRepository: Clone {
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<usize>;
    async fn search_files(&self, keyword: &str, limit: u64) -> AppResult<Vec<FileSearchRecord>>;
}
```

- [ ] **Step 2: Write failing service tests**

Create `app/src/application/file_index.rs` with these tests first:

```rust
use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct FileIndexService<R> {
    repo: R,
}

impl<R> FileIndexService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRepo {
        recorded: Arc<Mutex<Vec<FileIndexRecordInput>>>,
    }

    impl FileIndexRepository for FakeRepo {
        async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<usize> {
            self.recorded.lock().unwrap().extend_from_slice(files);
            Ok(files.len())
        }

        async fn search_files(
            &self,
            _keyword: &str,
            _limit: u64,
        ) -> AppResult<Vec<FileSearchRecord>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn record_seen_files_filters_files_without_hash_or_size() {
        let repo = FakeRepo::default();
        let service = FileIndexService::new(repo.clone());

        let written = service
            .record_seen_files(vec![
                SeenFile {
                    size: 0,
                    hash: SeenFileHash::Md5("abc".into()),
                    file_name: "zero.mkv".into(),
                    file_path: "/a".into(),
                },
                SeenFile {
                    size: 10,
                    hash: SeenFileHash::Unknown(String::new()),
                    file_name: "missing.mkv".into(),
                    file_path: "/a".into(),
                },
                SeenFile {
                    size: 20,
                    hash: SeenFileHash::Md5(" ABCDEF ".into()),
                    file_name: "movie.mkv".into(),
                    file_path: " /Movies ".into(),
                },
                SeenFile {
                    size: 30,
                    hash: SeenFileHash::Sha1(" 0123456789012345678901234567890123456789 ".into()),
                    file_name: "episode.mkv".into(),
                    file_path: "/Shows".into(),
                },
            ], Some(" desc ".into()))
            .await
            .unwrap();

        assert_eq!(written, 2);
        let recorded = repo.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].md5.as_deref(), Some("abcdef"));
        assert_eq!(recorded[0].sha1, None);
        assert_eq!(recorded[0].file_path, "/Movies");
        assert_eq!(recorded[0].description.as_deref(), Some("desc"));
        assert_eq!(
            recorded[1].sha1.as_deref(),
            Some("0123456789012345678901234567890123456789")
        );
    }

    #[test]
    fn hashes_location_with_version_and_null_separators() {
        assert_eq!(
            location_hash("/path", "file.mkv"),
            location_hash(" /path ", " file.mkv ")
        );
        assert_ne!(location_hash("/pa", "thfile"), location_hash("/path", "file"));
    }

    #[test]
    fn hashes_description_after_trim() {
        assert_eq!(description_hash(" hello "), description_hash("hello"));
        assert_ne!(description_hash("hello"), description_hash("Hello"));
    }
}
```

- [ ] **Step 3: Run focused test and verify RED**

Run:

```bash
cargo test -p bigbrother application::file_index
```

Expected: FAIL because `SeenFile`, `SeenFileHash`, `record_seen_files`, `location_hash`, and `description_hash` are missing.

- [ ] **Step 4: Implement service and hash helpers**

Replace the top of `app/src/application/file_index.rs` above the test module with:

```rust
use sha2::{Digest, Sha256};

use crate::{
    application::ports::{FileIndexRecordInput, FileIndexRepository, FileSearchRecord},
    error::AppResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenFileHash {
    Md5(String),
    Sha1(String),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenFile {
    pub size: u64,
    pub hash: SeenFileHash,
    pub file_name: String,
    pub file_path: String,
}

#[derive(Clone)]
pub struct FileIndexService<R> {
    repo: R,
}

impl<R> FileIndexService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R> FileIndexService<R>
where
    R: FileIndexRepository,
{
    pub async fn record_seen_files(
        &self,
        files: Vec<SeenFile>,
        description: Option<String>,
    ) -> AppResult<usize> {
        let description = normalize_optional_text(description);
        let inputs = files
            .into_iter()
            .filter_map(|file| to_record_input(file, description.clone()))
            .collect::<Vec<_>>();

        self.repo.record_files(&inputs).await
    }

    pub async fn search_files(
        &self,
        keyword: &str,
        limit: u64,
    ) -> AppResult<Vec<FileSearchRecord>> {
        self.repo.search_files(keyword.trim(), limit).await
    }
}

pub fn location_hash(file_path: &str, file_name: &str) -> String {
    hash_hex(format!(
        "v1\0{}\0{}",
        file_path.trim(),
        file_name.trim()
    ))
}

pub fn description_hash(description: &str) -> String {
    hash_hex(description.trim())
}

fn to_record_input(
    file: SeenFile,
    description: Option<String>,
) -> Option<FileIndexRecordInput> {
    if file.size == 0 {
        return None;
    }

    let (md5, sha1) = match file.hash {
        SeenFileHash::Md5(value) => (normalize_hash(value), None),
        SeenFileHash::Sha1(value) => (None, normalize_hash(value)),
        SeenFileHash::Unknown(value) => match normalize_hash(value) {
            Some(hash) if hash.len() == 32 => (Some(hash), None),
            Some(hash) if hash.len() == 40 => (None, Some(hash)),
            _ => (None, None),
        },
    };

    if md5.is_none() && sha1.is_none() {
        return None;
    }

    Some(FileIndexRecordInput {
        size: file.size,
        md5,
        sha1,
        file_name: file.file_name.trim().to_owned(),
        file_path: file.file_path.trim().to_owned(),
        description,
    })
}

fn normalize_hash(value: String) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn hash_hex(value: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(value.as_ref());
    hex::encode(digest)
}
```

Keep the test module below this code and remove duplicate imports from the initial skeleton.

- [ ] **Step 5: Run focused test and verify GREEN**

Run:

```bash
cargo test -p bigbrother application::file_index
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml app/Cargo.toml app/src/application/file_index.rs app/src/application/ports.rs app/src/application/mod.rs
git commit -m "add file index application model"
```

## Task 3: Implement SeaORM File Index Repository

**Files:**
- Create: `app/src/infrastructure/entity/file_index.rs`
- Create: `app/src/infrastructure/repo/file_index.rs`
- Modify: `app/src/infrastructure/entity/mod.rs`
- Modify: `app/src/infrastructure/repo/mod.rs`

- [ ] **Step 1: Export modules**

Edit `app/src/infrastructure/entity/mod.rs`:

```rust
pub mod file_index;
```

Edit `app/src/infrastructure/repo/mod.rs`:

```rust
pub mod file_index;
```

- [ ] **Step 2: Write repository integration tests**

Create `app/src/infrastructure/repo/file_index.rs` with:

```rust
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
    async fn record_files(&self, files: &[FileIndexRecordInput]) -> AppResult<usize> {
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
                md5: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                sha1: None,
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: Some("same desc".into()),
            },
            FileIndexRecordInput {
                size: 100,
                md5: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                sha1: None,
                file_name: "movie.mkv".into(),
                file_path: "/Movies".into(),
                description: Some("same desc".into()),
            },
        ];

        assert_eq!(repo.record_files(&files).await.unwrap(), 2);
        assert_eq!(repo.record_files(&files).await.unwrap(), 2);

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].descriptions, vec!["same desc"]);
    }

    #[tokio::test]
    async fn record_files_keeps_multiple_locations_for_same_file() {
        let repo = repo().await;
        repo.record_files(&[
            FileIndexRecordInput {
                size: 100,
                md5: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                sha1: None,
                file_name: "movie-a.mkv".into(),
                file_path: "/A".into(),
                description: Some("desc".into()),
            },
            FileIndexRecordInput {
                size: 100,
                md5: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
                sha1: None,
                file_name: "movie-b.mkv".into(),
                file_path: "/B".into(),
                description: Some("desc".into()),
            },
        ])
        .await
        .unwrap();

        let results = repo.search_files("movie", 20).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_files_matches_description() {
        let repo = repo().await;
        repo.record_files(&[FileIndexRecordInput {
            size: 200,
            md5: None,
            sha1: Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()),
            file_name: "episode.mkv".into(),
            file_path: "/Shows".into(),
            description: Some("rare keyword".into()),
        }])
        .await
        .unwrap();

        let results = repo.search_files("rare", 20).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sha1.as_deref(), Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }
}
```

- [ ] **Step 3: Run focused test and verify RED**

Run:

```bash
cargo test -p bigbrother infrastructure::repo::file_index
```

Expected: FAIL because `entity::file_index::record_files` and `search_files` are missing.

- [ ] **Step 4: Implement entity helpers**

Create `app/src/infrastructure/entity/file_index.rs`:

```rust
use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
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
            file_location::Column::FileName
                .contains(keyword.trim())
                .or(file_location::Column::FilePath.contains(keyword.trim())),
        )
        .order_by_asc(file_location::Column::Id)
        .limit(limit)
        .all(db)
        .await?;

    let mut by_location = BTreeMap::new();
    for location in locations {
        if let Some(record) = record_for_location(db, &location).await? {
            by_location.insert(location.id, record);
        };
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
            if !by_location.contains_key(&location.id) {
                if let Some(record) = record_for_location(db, &location).await? {
                    by_location.insert(location.id, record);
                }
            }
            if let Some(record) = by_location.get_mut(&location.id) {
                push_unique(&mut record.descriptions, description.description.clone());
            }
        }
    }

    for (location_id, record) in by_location.iter_mut() {
        let descriptions = descriptions_for_location(db, *location_id).await?;
        for description in descriptions {
            push_unique(&mut record.descriptions, description);
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
        size: index.size,
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
            .filter(file_index::Column::Size.eq(file.size))
            .filter(file_index::Column::Md5.eq(md5))
            .one(db)
            .await?
    {
        update_missing_hashes(db, existing.id, file).await?;
        return Ok(existing.id);
    }

    if let Some(sha1) = file.sha1.as_deref()
        && let Some(existing) = file_index::Entity::find()
            .filter(file_index::Column::Size.eq(file.size))
            .filter(file_index::Column::Sha1.eq(sha1))
            .one(db)
            .await?
    {
        update_missing_hashes(db, existing.id, file).await?;
        return Ok(existing.id);
    }

    let now = Utc::now();
    let inserted = file_index::ActiveModel {
        size: ActiveValue::Set(file.size),
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

async fn update_missing_hashes<C>(db: &C, id: i64, file: &FileIndexRecordInput) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let Some(existing) = file_index::Entity::find_by_id(id).one(db).await? else {
        return Ok(());
    };
    let mut active: file_index::ActiveModel = existing.into();
    let mut changed = false;
    if active.md5.as_ref() == &None && file.md5.is_some() {
        active.md5 = ActiveValue::Set(file.md5.clone());
        changed = true;
    }
    if active.sha1.as_ref() == &None && file.sha1.is_some() {
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
```

- [ ] **Step 5: Run focused repository tests**

Run:

```bash
cargo test -p bigbrother infrastructure::repo::file_index
```

Expected: PASS for all repository tests.

- [ ] **Step 6: Commit**

```bash
git add app/src/infrastructure/entity/file_index.rs app/src/infrastructure/entity/mod.rs app/src/infrastructure/repo/file_index.rs app/src/infrastructure/repo/mod.rs
git commit -m "add file index repository"
```

## Task 4: Add Ingest Source Parsing Service

**Files:**
- Modify: `app/src/application/file_index.rs`
- Modify: `app/src/application/ports.rs`
- Modify: `app/src/application/import/share/providers.rs`
- Modify: `app/src/application/import/json.rs`
- Modify: `app/src/application/import_media.rs`

- [ ] **Step 1: Add ingest source types and conversion helper tests**

Append tests to `app/src/application/file_index.rs`:

```rust
#[cfg(test)]
mod ingest_tests {
    use super::*;
    use crate::domain::import::inner::{Etag, RawFile};

    #[test]
    fn raw_file_conversion_preserves_path_name_size_and_hash() {
        let file = SeenFile::from_raw_file(&RawFile {
            id: None,
            name: "movie.mkv".into(),
            etag: Etag::Md5("ABC".into()),
            size: 100,
            path: "/Movies".into(),
        });

        assert_eq!(file.file_name, "movie.mkv");
        assert_eq!(file.file_path, "/Movies");
        assert_eq!(file.size, 100);
        assert_eq!(file.hash, SeenFileHash::Md5("ABC".into()));
    }
}
```

- [ ] **Step 2: Run focused test and verify RED**

Run:

```bash
cargo test -p bigbrother application::file_index::ingest_tests
```

Expected: FAIL because `SeenFile::from_raw_file` is missing.

- [ ] **Step 3: Implement raw file conversion and source enum**

Add to `app/src/application/file_index.rs`:

```rust
use crate::domain::import::inner::{Etag, RawFile};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileIndexSource {
    ShareUrl(String),
    Fslink(String),
    LocalJsonFile(String),
}

impl SeenFile {
    pub fn from_raw_file(file: &RawFile) -> Self {
        let hash = match &file.etag {
            Etag::Md5(value) => SeenFileHash::Md5(value.clone()),
            Etag::Sha1(value) => SeenFileHash::Sha1(value.clone()),
        };

        Self {
            size: file.size,
            hash,
            file_name: file.name.clone(),
            file_path: file.path.clone(),
        }
    }
}
```

- [ ] **Step 4: Extract raw JSON/fslink helpers**

In `app/src/application/import/json.rs`, add public helper methods to `JsonImportUseCase`:

```rust
pub(crate) fn raw_files_from_fslink(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
    let resource = self.parse_fslink_resource(fslink)?;
    Ok(raw_files_from_resource(&resource))
}

pub(crate) fn raw_files_from_json(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
    let resource: ResourceJson = parse_files_from_json(json)?;
    Ok(raw_files_from_resource(&resource))
}
```

Move the raw-file construction loop from `normalize_resource_files` into a module function:

```rust
pub(crate) fn raw_files_from_resource(resource: &ResourceJson) -> Vec<RawFile> {
    let mut raw_files = Vec::new();

    for file in &resource.files {
        let full_path = format!("{}/{}", &resource.common_path, &file.path);
        let path = Path::new(full_path.as_str());
        let parent_path = path
            .parent()
            .map(|p| p.to_str().unwrap_or_default())
            .unwrap_or_default();
        let name = path
            .file_name()
            .map(|p| p.to_str().unwrap_or_default())
            .unwrap_or_default();

        raw_files.push(RawFile {
            id: None,
            name: name.to_owned(),
            etag: file.etag.as_str().into(),
            size: file.size,
            path: parent_path.to_owned(),
        });
    }

    raw_files
}
```

Then make `normalize_resource_files` call:

```rust
let raw_files = raw_files_from_resource(resource);
self.metadata_lookup_mut().build_media_files(raw_files)
```

- [ ] **Step 5: Extract raw share helpers**

In `app/src/application/import/share/providers.rs`, add methods that return raw files before metadata grouping:

```rust
pub(crate) async fn raw_files_from_pan123_share(
    &mut self,
    share_key: &str,
    share_password: &str,
) -> AppResult<Vec<crate::domain::import::inner::RawFile>> {
    let mut traversal = ShareTraversal::new((0, String::new()));
    while let Some((parent_id, parent_path)) = traversal.next_dir() {
        let files = self
            .share_source()
            .list_pan123_share_files(share_key, share_password, parent_id)
            .await?;
        traversal.extend(collect_pan123_directory_entries(&files, &parent_path));
    }
    Ok(traversal.into_raw_files())
}
```

Make `list_files_from_pan123_share` call this helper and then `build_media_files`. Repeat the same pattern for pan115 and pan189 while preserving the existing pan189 CAS behavior.

- [ ] **Step 6: Add `FileIndexIngestService`**

Add to `app/src/application/file_index.rs`:

```rust
use url::Url;

use crate::{
    application::import::{
        ImportLocalStore, ImportMediaService, LibraryGateway, MetadataCatalog, ShareSource,
        ShareUrl,
    },
    error::AppError,
};

pub trait FileIndexRawFileSource: Clone {
    async fn raw_files_from_share_url_string(&self, url: &str) -> AppResult<Vec<RawFile>>;
    async fn raw_files_from_fslink_string(&self, fslink: &str) -> AppResult<Vec<RawFile>>;
    async fn raw_files_from_json_bytes(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>>;
}

impl<L, S, M, F> FileIndexRawFileSource for ImportMediaService<L, S, M, F>
where
    L: LibraryGateway,
    S: ShareSource,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    async fn raw_files_from_share_url_string(&self, raw_url: &str) -> AppResult<Vec<RawFile>> {
        let url = Url::parse(raw_url).map_err(|err| {
            AppError::InvalidParameter(format!("invalid share url '{raw_url}': {err}"))
        })?;
        let share = ShareUrl::from(&url).ok_or_else(|| {
            AppError::InvalidParameter(format!(
                "unsupported share url '{raw_url}', expected pan123, pan189, or pan115 share link"
            ))
        })?;
        self.raw_files_from_share_url(&share).await
    }

    async fn raw_files_from_fslink_string(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
        self.raw_files_from_fslink(fslink).await
    }

    async fn raw_files_from_json_bytes(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
        self.raw_files_from_json(json).await
    }
}

#[derive(Clone)]
pub struct FileIndexIngestService<I, R> {
    source: I,
    file_index: FileIndexService<R>,
}

impl<I, R> FileIndexIngestService<I, R> {
    pub fn new(source: I, file_index: FileIndexService<R>) -> Self {
        Self { source, file_index }
    }
}
```

In `app/src/application/import_media.rs`, add raw list methods to `ImportMediaService`:

```rust
pub async fn raw_files_from_share_url(
    &self,
    url: &ShareUrl<'_>,
) -> AppResult<Vec<crate::domain::import::inner::RawFile>> {
    self.import_use_cases
        .share_import()
        .raw_files_from_share_url(url)
        .await
}

pub async fn raw_files_from_fslink(
    &self,
    fslink: &str,
) -> AppResult<Vec<crate::domain::import::inner::RawFile>> {
    self.import_use_cases
        .json_import()
        .raw_files_from_fslink(fslink)
}

pub async fn raw_files_from_json(
    &self,
    json: Vec<u8>,
) -> AppResult<Vec<crate::domain::import::inner::RawFile>> {
    self.import_use_cases
        .json_import()
        .raw_files_from_json(json)
}
```

Then implement ingest methods on `FileIndexIngestService<I, R>`:

```rust
impl<I, R> FileIndexIngestService<I, R>
where
    I: FileIndexRawFileSource,
    R: FileIndexRepository,
{
    pub async fn ingest_sources(
        &self,
        sources: Vec<FileIndexSource>,
        description: Option<String>,
    ) -> AppResult<usize> {
        let mut total = 0;
        for source in sources {
            let raw_files = match source {
                FileIndexSource::ShareUrl(raw_url) => {
                    self.source.raw_files_from_share_url_string(&raw_url).await?
                }
                FileIndexSource::Fslink(fslink) => {
                    self.source.raw_files_from_fslink_string(&fslink).await?
                }
                FileIndexSource::LocalJsonFile(path) => {
                    let json = tokio::fs::read(&path).await.map_err(|err| {
                        AppError::Runtime(format!(
                            "failed to read local index source '{path}': {err}"
                        ))
                    })?;
                    self.source.raw_files_from_json_bytes(json).await?
                }
            };
            let seen = raw_files.iter().map(SeenFile::from_raw_file).collect();
            total += self
                .file_index
                .record_seen_files(seen, description.clone())
                .await?;
        }
        Ok(total)
    }
}
```

- [ ] **Step 7: Run existing import tests**

Run:

```bash
cargo test -p bigbrother application::import_media
```

Expected: PASS. Existing import behavior must not change.

- [ ] **Step 8: Commit**

```bash
git add app/src/application/file_index.rs app/src/application/ports.rs app/src/application/import/share/providers.rs app/src/application/import/json.rs app/src/application/import_media.rs
git commit -m "add file index ingest parsing"
```

## Task 5: Wire CLI Synchronous Indexing and Search

**Files:**
- Modify: `app/src/interface/cli/mod.rs`
- Modify: `app/src/main.rs`
- Modify: `app/src/bootstrap/services.rs`

- [ ] **Step 1: Write CLI parse tests**

Append tests to `app/src/interface/cli/mod.rs`:

```rust
#[test]
fn parses_import_share_url_description() {
    let cli = Cli::parse_from([
        "bigbrother",
        "import-share-url",
        "--description",
        "from cli",
        "--data-dir",
        "./data",
        "https://www.123pan.com/s/test?pwd=pass",
    ]);

    match cli.command {
        Commands::ImportShareUrl(args) => {
            assert_eq!(args.description.as_deref(), Some("from cli"));
        }
        _ => panic!("expected import-share-url command"),
    }
}

#[test]
fn parses_search_files_command() {
    let cli = Cli::parse_from([
        "bigbrother",
        "search-files",
        "--limit",
        "50",
        "--data-dir",
        "./data",
        "movie",
    ]);

    match cli.command {
        Commands::SearchFiles(args) => {
            assert_eq!(args.keyword, "movie");
            assert_eq!(args.limit, 50);
        }
        _ => panic!("expected search-files command"),
    }
}
```

- [ ] **Step 2: Run CLI tests and verify RED**

Run:

```bash
cargo test -p bigbrother interface::cli
```

Expected: FAIL because `description` and `SearchFiles` are missing.

- [ ] **Step 3: Extend CLI structs**

Edit `app/src/interface/cli/mod.rs`:

```rust
#[derive(Subcommand)]
pub enum Commands {
    Server(DataDirArgs),
    ImportShareUrl(ImportShareUrlArgs),
    SearchFiles(SearchFilesArgs),
}

#[derive(Args)]
pub struct ImportShareUrlArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    pub url: String,
}

#[derive(Args)]
pub struct SearchFilesArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long, default_value_t = 20)]
    pub limit: u64,
    pub keyword: String,
}
```

- [ ] **Step 4: Add service builders**

Edit `app/src/bootstrap/services.rs` and add imports for `FileIndexService`, `FileIndexIngestService`, and `SeaOrmFileIndexRepository`.

Add aliases:

```rust
pub(crate) type FileIndexRuntimeService = FileIndexService<SeaOrmFileIndexRepository>;
pub(crate) type FileIndexIngestRuntimeService = FileIndexIngestService<ImportService, SeaOrmFileIndexRepository>;
```

Add builders:

```rust
pub(crate) fn build_file_index_service(db: sea_orm::DatabaseConnection) -> FileIndexRuntimeService {
    FileIndexService::new(SeaOrmFileIndexRepository::new(db))
}

pub(crate) fn build_file_index_ingest_service(
    config: &config::Manager,
    db: sea_orm::DatabaseConnection,
) -> FileIndexIngestRuntimeService {
    FileIndexIngestService::new(
        build_import_service(config),
        build_file_index_service(db),
    )
}
```

- [ ] **Step 5: Wire main command handling**

Edit `app/src/main.rs`:

```rust
Commands::SearchFiles(args) => {
    if let Err(err) = run_search_files(args.data_dir.data_dir.as_str(), &args.keyword, args.limit).await {
        eprintln!("Failed to search files: {err}");
        std::process::exit(1);
    }
}
```

In `run_import_share_url`, after building `import_service`, build DB and ingest service:

```rust
let app = AppContext::new(data_dir).await?;
let db = app.runtime_inputs().db;
let ingest_service = bootstrap::services::build_file_index_ingest_service(&config, db);
if let Err(err) = ingest_service
    .ingest_sources(
        vec![application::file_index::FileIndexSource::ShareUrl(url.to_string())],
        args_description.clone(),
    )
    .await
{
    eprintln!("Warning: failed to index share url: {err}");
}
```

Adjust `run_import_share_url` signature:

```rust
async fn run_import_share_url(
    data_dir: &str,
    url: &str,
    verbose: bool,
    description: Option<String>,
) -> AppResult<()>
```

Add `run_search_files`:

```rust
async fn run_search_files(data_dir: &str, keyword: &str, limit: u64) -> AppResult<()> {
    let app = AppContext::new(data_dir).await?;
    let service = bootstrap::services::build_file_index_service(app.runtime_inputs().db);
    let results = service.search_files(keyword, limit).await?;
    if results.is_empty() {
        println!("未找到匹配文件");
        return Ok(());
    }

    for (index, record) in results.iter().enumerate() {
        println!("{}. {}", index + 1, record.file_name);
        println!("   path: {}", record.file_path);
        println!("   size: {}", record.size);
        if let Some(md5) = &record.md5 {
            println!("   md5: {md5}");
        }
        if let Some(sha1) = &record.sha1 {
            println!("   sha1: {sha1}");
        }
        for description in record.descriptions.iter().take(3) {
            println!("   description: {description}");
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Run CLI tests**

Run:

```bash
cargo test -p bigbrother interface::cli
```

Expected: PASS.

- [ ] **Step 7: Run cargo check**

Run:

```bash
cargo check -p bigbrother
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add app/src/interface/cli/mod.rs app/src/main.rs app/src/bootstrap/services.rs
git commit -m "add file index cli commands"
```

## Task 6: Add Telegram Index Event Publishing and Worker

**Files:**
- Create: `app/src/interface/telegram/file_index.rs`
- Modify: `app/src/interface/telegram/mod.rs`
- Modify: `app/src/interface/telegram/msg.rs`
- Modify: `app/src/bootstrap/app.rs`
- Modify: `app/src/bootstrap/mod.rs`

- [ ] **Step 1: Add event payload and extraction tests**

Create `app/src/interface/telegram/file_index.rs`:

```rust
use serde::{Deserialize, Serialize};
use teloxide::types::{InlineKeyboardButtonKind, Message, MessageEntityKind};
use url::Url;

use crate::{
    application::file_index::FileIndexSource,
    infrastructure::event_bus::Event,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFilesFromSource {
    pub sources: Vec<FileIndexSource>,
    pub description: Option<String>,
    pub source_kind: String,
}

impl Event for IndexFilesFromSource {
    const NAME: &'static str = "IndexFilesFromSource";
}

pub fn extract_index_sources(msg: &Message) -> Vec<FileIndexSource> {
    let text = msg.text().or(msg.caption()).unwrap_or_default();
    let urls = extract_urls(msg)
        .into_iter()
        .map(|url| url.to_string())
        .collect::<Vec<_>>();
    extract_index_sources_from_parts(text, urls)
}

pub fn extract_index_sources_from_parts(
    text: &str,
    raw_urls: Vec<String>,
) -> Vec<FileIndexSource> {
    let mut sources = Vec::new();
    for line in text.lines() {
        if crate::application::import::is_fslink(line) {
            sources.push(FileIndexSource::Fslink(line.to_owned()));
        }
    }
    for raw_url in raw_urls {
        let Ok(url) = Url::parse(&raw_url) else {
            continue;
        };
        if crate::application::import::ShareUrl::from(&url).is_some() {
            sources.push(FileIndexSource::ShareUrl(url.to_string()));
        }
    }
    sources
}

pub fn message_description(msg: &Message) -> Option<String> {
    msg.text()
        .or(msg.caption())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn extract_urls(msg: &Message) -> Vec<Url> {
    let mut urls = Vec::new();
    let text = msg.text().unwrap_or_default();
    super::msg::extract_urls_from_text(text, &mut urls);

    if let Some(entities) = msg.caption_entities() {
        for entity in entities {
            if let MessageEntityKind::TextLink { url } = &entity.kind {
                urls.push(url.clone());
            }
        }
    }

    if let Some(reply_markup) = msg.reply_markup() {
        for buttons in &reply_markup.inline_keyboard {
            for button in buttons {
                if let InlineKeyboardButtonKind::Url(url) = &button.kind {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls
}
```

Add tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_index_sources_from_text_parts_keeps_share_and_fslink() {
        let sources = extract_index_sources_from_parts(
            "123FSLinkV2$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#100#movie.mkv",
            vec!["https://www.123pan.com/s/test?pwd=pass"],
        );

        assert_eq!(sources.len(), 2);
    }
}
```

- [ ] **Step 2: Make Telegram URL extraction reusable**

In `app/src/interface/telegram/msg.rs`, change `extract_urls_from_text` from a private method to a module function:

```rust
pub(super) fn extract_urls_from_text(text: &str, urls: &mut Vec<Url>) {
    if text.is_empty() {
        return;
    }

    for cap in URL_RE.captures_iter(text) {
        if let Some(matched_url) = cap.get(0)
            && let Ok(url) = Url::parse(matched_url.as_str())
        {
            urls.push(url);
        }
    }
}
```

Then update the method call:

```rust
extract_urls_from_text(text, &mut urls);
```

- [ ] **Step 3: Wire Telegram runtime to publish event**

In `app/src/interface/telegram/mod.rs`, export module:

```rust
pub mod file_index;
```

Add an `EventBus` field to `BotRuntime` through its services so both `handle_channel_post` and `handle_message` can publish `IndexFilesFromSource`.

Update imports in `app/src/interface/telegram/mod.rs`:

```rust
use crate::infrastructure::event_bus::EventBus;
```

Add this field to `BotServices`:

```rust
file_index_events: EventBus,
```

Add this constructor parameter to `BotRuntime::new`:

```rust
file_index_events: EventBus,
```

Store it in `BotServices`:

```rust
file_index_events,
```

Add this accessor:

```rust
fn file_index_event_bus(&self) -> &EventBus {
    &self.services.file_index_events
}
```

Add helper:

```rust
async fn publish_file_index_event(runtime: &BotRuntime, msg: &Message) {
    let sources = file_index::extract_index_sources(msg);
    if sources.is_empty() {
        return;
    }
    let event = file_index::IndexFilesFromSource {
        sources,
        description: file_index::message_description(msg),
        source_kind: "telegram".to_owned(),
    };
    if let Err(err) = runtime.file_index_event_bus().publish(&event).await {
        error!("Failed to publish file index event: {}", err);
    }
}
```

Call this helper at the beginning of both `handle_channel_post` and authorized `handle_message`, before keyword filtering and before `MsgProcessor::process()`.

In `app/src/bootstrap/mod.rs`, update the `BotRuntime::new(...)` call by passing `event_bus.clone()` as the last argument.

- [ ] **Step 4: Wire event worker**

In `app/src/bootstrap/mod.rs`, add a field to `EventDeliveryRuntime`:

```rust
pub file_index_ingest: FileIndexIngestRuntimeService,
```

Initialize it in `AppRuntime::from_app`:

```rust
file_index_ingest: FileIndexIngestService::new(
    build_import_service_from_clients(
        &inputs.clients,
        inputs.import_remote_path.clone(),
        inputs.import_local_path.clone(),
        inputs.import_strm_download_url.clone(),
    ),
    build_file_index_service(inputs.db.clone()),
),
```

In `EventDeliveryRuntime::run`, subscribe:

```rust
self.event_bus
    .subscribe(self.file_index_ingest.clone(), on_index_files_from_source)
    .await?;
```

Add handler:

```rust
async fn on_index_files_from_source(
    service: FileIndexIngestRuntimeService,
    payload: crate::interface::telegram::file_index::IndexFilesFromSource,
) -> AppResult<()> {
    service.ingest_sources(payload.sources, payload.description).await?;
    Ok(())
}
```

- [ ] **Step 5: Handle Telegram document local source**

Add `ingest_dir` to `RuntimeBootstrapInputs` in `app/src/bootstrap/app.rs`:

```rust
pub file_index_ingest_dir: String,
```

Initialize:

```rust
file_index_ingest_dir: format!("{data_dir}/ingest/file-index"),
```

In Telegram document handling, before import processing, download JSON/CAS document to the ingest dir and add `FileIndexSource::LocalJsonFile(path)` to the event. Use `tokio::fs::create_dir_all` and a filename containing Telegram file unique id plus the original extension.

- [ ] **Step 6: Run Telegram tests/check**

Run:

```bash
cargo test -p bigbrother interface::telegram
cargo check -p bigbrother
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add app/src/interface/telegram/file_index.rs app/src/interface/telegram/mod.rs app/src/interface/telegram/msg.rs app/src/bootstrap/app.rs app/src/bootstrap/mod.rs
git commit -m "index telegram resources asynchronously"
```

## Task 7: Final Verification and Cleanup

**Files:**
- Review all files changed in Tasks 1-6.

- [ ] **Step 1: Run full tests**

Run:

```bash
make test
```

Expected: PASS.

- [ ] **Step 2: Run lint**

Run:

```bash
make lint
```

Expected: PASS.

- [ ] **Step 3: Review changed files**

Run:

```bash
git status --short
git diff --stat HEAD
```

Expected: only intended file-index implementation files are changed since the last task commit.

- [ ] **Step 4: Commit final fixes if any**

If `make test` or `make lint` required fixes, commit them:

```bash
git add Cargo.toml app/src migration/src
git commit -m "stabilize file index implementation"
```

If no files changed after verification, skip this commit.

## Self-Review Notes

- Spec coverage: schema, hash de-duplication, location hash, description de-duplication, CLI synchronous indexing, CLI search, Telegram async event indexing, no Telegram metadata storage, and existing import preservation each have a task.
- Scope: one feature with database, application, CLI, and Telegram integration. It is broad but produces one cohesive searchable file index.
- Repository query shape: Task 3 uses explicit ID lookups instead of SeaORM relation helpers, so the plan does not require adding relation definitions to generated-style entity models.
- Verification gates: each task has focused tests or compile checks; Task 7 runs full `make test` and `make lint`.
