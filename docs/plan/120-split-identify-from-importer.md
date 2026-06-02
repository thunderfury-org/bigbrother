# Plan: refactor — split media identify from importer (issue #120)

## Context

This is the prerequisite refactor for **issue #121** (subscription as sole import gate). Today `application/import/factory.rs::TransferWorkflow<L, M, F, T>` is monolithic: it owns `TmdbLookup` + `MetadataLookup`, runs identify/group/transfer all inside one `transfer_media_files(&[MediaFile])` call, and emits `Vec<ImportedMedia>`.

#121 needs to insert a subscription filter **between** identify (which resolves `tmdb_id`) and transfer (which moves files). That insertion point doesn't exist today — there is no clean boundary between "what TMDB says this is" and "go put it in the library".

This refactor carves out that boundary:

```
RawFile → MetadataLookup → MediaFile → MediaIdentifyService → IdentifyOutcome { groups, unmatched }
                                                            → MediaImporter (transfer only)
```

**Behavior must not change.** All existing tests pass without semantic edits; only the call shape moves.

`CONTEXT.md`'s glossary is unaffected — Subscription term is parked on `docs/subscription-context-term` for #121.

## Approach overview

Five sequential TDD steps, each leaving the tree green:

1. **Add `MediaIdentifyService` in parallel** to `group.rs` (still borrowed `Media<'a>`). Unit-test it.
2. **Wire `TransferWorkflow` to call identify internally** (replacing `group_media_files`). Delete `group.rs`.
3. **Convert `Media` to owned** (drop `'a`). Patch `domain/import/policy.rs::insert_*_media` and `transfer/` consumers (build temporary ref views via small helpers).
4. **Extract identify from `TransferWorkflow`.** Drop `<M, T>` generics. Rename trait method to `import_groups(Vec<Media>, Vec<UnmatchedFile>)`. Update 3 call sites + test fakes + `infrastructure/services.rs` type aliases.
5. **Cleanup** — unused imports, `make lint`, `make test`.

Each step is a separate commit; the branch is `refactor/split-identify-from-importer`.

## Step-by-step

### Step 1 — Introduce `MediaIdentifyService` (parallel to group)

**New file**: `app/src/application/import/identify.rs`

```rust
pub struct MediaIdentifyService<M, T> {
    tmdb_lookup: TmdbLookup<M, T>,
}

#[derive(Debug)]
pub struct IdentifyOutcome<'a> {
    pub groups: Vec<Media<'a>>,
    pub unmatched: Vec<UnmatchedFile>,
}

#[derive(Debug, Clone)]
pub struct UnmatchedFile {
    pub file_name: String,
    pub file_path: String,
}

impl<M, T> MediaIdentifyService<M, T>
where
    M: MetadataCatalog,
    T: TitleExtractor,
{
    pub fn new(metadata_catalog: M, title_extractor: T) -> Self {
        Self { tmdb_lookup: TmdbLookup::new(metadata_catalog, title_extractor) }
    }

    pub async fn identify<'a>(
        &mut self,
        files: &'a [MediaFile],
    ) -> AppResult<IdentifyOutcome<'a>> { ... }
}
```

Body lifts `group_media_files` logic verbatim from `group.rs:21-46`, replacing `(grouped, unmatched: Vec<(&str, &str)>)` returns with `IdentifyOutcome { groups, unmatched: Vec<UnmatchedFile> }`. Note this step still uses **borrowed** `Media<'a>` — owned conversion is step 3.

**Derive `Clone`** on `MediaIdentifyService<M, T>` so handler/CLI structs can clone it (mirrors `TransferWorkflow`'s existing Clone).

**Tests in `app/src/application/import/identify/tests.rs`** (sibling pattern matches `group/tests.rs`):
- `identify_groups_tv_files_by_tmdb_id` — two TV files of same series end up in one `Media::Tv` with correct season/episode nesting
- `identify_groups_movie_files_by_tmdb_id`
- `identify_returns_unmatched_when_tmdb_returns_none` — file flows to `unmatched` with name + path
- `identify_returns_unmatched_when_episode_slot_unresolved` — TV file where `resolve_tv_episode_slot` returns None
- Reuse existing `FakeMetadataCatalog` / `FakeTitleExtractor` test doubles from `tmdb_info/tests.rs`

**Module wiring** — add `mod identify;` to `application/import/mod.rs` and re-export `pub use identify::{MediaIdentifyService, IdentifyOutcome, UnmatchedFile};`.

**Green criterion**: new tests pass; existing tests untouched; no production callers wired yet.

### Step 2 — Route `TransferWorkflow` through identify, delete `group.rs`

In `application/import/factory.rs`, add `identify_service: MediaIdentifyService<M, T>` field to `TransferWorkflow` (constructed in `new()` from the existing `metadata_catalog` + `title_extractor` params). Keep `tmdb_lookup` field for now (still used by `parse.rs::ParseService` indirectly? — verify; if no other user remains, remove).

In `application/import/transfer.rs`, replace the `group_media_files` call inside `build_import_plan` with `self.identify_service.identify(media_files).await?`. Convert `IdentifyOutcome` back to the (groups, unmatched_refs) shape the rest of `transfer.rs` currently expects — this is a temporary adapter, removed in step 4.

**Delete `application/import/group.rs`**. Move `group/tests.rs` content into `identify/tests.rs` (already created in step 1) and adjust the `resolve_tv_episode_slot` import — landing on `crate::domain::import::policy::resolve_tv_episode_slot` (where the actual definition lives — `group.rs` was just re-exporting).

Remove `pub mod group;` from `application/import/mod.rs`.

**Green criterion**: full test suite passes. Production code path now goes through `MediaIdentifyService` even though the trait signature hasn't changed yet.

### Step 3 — Convert `Media` to owned

This is the borrow-checker step.

**`app/src/domain/import/inner.rs`** — change:
```rust
pub enum Media<'a> {
    Movie { detail: MovieDetail, files: Vec<&'a MediaFile> },
    Tv    { detail: TvDetail, files: BTreeMap<u32, BTreeMap<u32, Vec<&'a MediaFile>>> },
}
```
to:
```rust
pub enum Media {
    Movie { detail: MovieDetail, files: Vec<MediaFile> },
    Tv    { detail: TvDetail, files: BTreeMap<u32, BTreeMap<u32, Vec<MediaFile>>> },
}
```

**`app/src/domain/import/policy.rs`**:
- `insert_movie_media(grouped: &mut HashMap<u32, Media>, detail: MovieDetail, file: MediaFile)` — drop `'a`, take owned.
- `insert_tv_media(grouped: &mut HashMap<u32, Media>, detail: TvDetail, season: u32, episode: u32, file: MediaFile)` — same.
- `resolve_tv_episode_slot` unchanged (already `&MediaFile`).

**`app/src/application/import/identify.rs`** — change `identify` to take `Vec<MediaFile>` (move) and produce `IdentifyOutcome` with no `'a`. Iterate `files.into_iter()` and move each `MediaFile` into either `insert_*_media` or into `unmatched` (using owned `String` file_name / file_path).

**`app/src/application/import/transfer.rs` and `transfer/` submodules** — transfer functions today take `Vec<&MediaFile>` (movie) and `BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>` (tv). Keep those signatures. In `execute_import_plan`, build temporary ref views from the now-owned `Media`:

```rust
match media {
    Media::Movie { detail, files } => {
        let refs: Vec<&MediaFile> = files.iter().collect();
        self.transfer_movie(detail, &refs).await?;
    }
    Media::Tv { detail, files } => {
        let refs: BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>> = borrow_tv_files(&files);
        self.transfer_tv(detail, &refs).await?;
    }
}
```

Add small helper `fn borrow_tv_files(files: &BTreeMap<u32, BTreeMap<u32, Vec<MediaFile>>>) -> BTreeMap<u32, BTreeMap<u32, Vec<&MediaFile>>>` either in `transfer.rs` or a private util. This keeps `transfer/movie.rs`, `transfer/tv.rs`, `transfer/episode.rs`, `transfer/season.rs`, `transfer_target.rs` **untouched**.

**Green criterion**: full test suite passes. Data is now owned end-to-end.

### Step 4 — Extract identify from `TransferWorkflow`, change trait, update call sites

**`app/src/application/import/factory.rs`**:
- Drop `identify_service` field, drop `tmdb_lookup` field (verified unused outside identify), drop `metadata_lookup` field (verified production callers maintain their own — see Plan agent's Q2).
- Drop `<M, T>` generics → `TransferWorkflow<L, F>`.
- `TransferWorkflow::new(library_gateway: L, local: F)` — simpler ctor.
- Prune `<M, T>` bounds from every `impl<L, M, F, T> TransferWorkflow<L, M, F, T>` block in `transfer.rs`, `transfer/movie.rs`, `transfer/tv.rs`, `transfer/episode.rs`, `transfer/season.rs`, `transfer_target.rs`, `library.rs`, `transfer_save.rs`, `transfer_cleanup.rs`, `transfer_support/*`.

**`app/src/application/import_ports.rs`** — update `MediaImporter`:
```rust
pub trait MediaImporter: Send + 'static {
    fn import_groups(
        &mut self,
        groups: Vec<Media>,
        unmatched: Vec<UnmatchedFile>,
    ) -> impl std::future::Future<Output = AppResult<Vec<ImportedMedia>>> + Send;
}
```

**`TransferWorkflow::import_groups`** in `transfer.rs` — accepts `(groups, unmatched)`, calls `execute_import_plan` on groups, appends a single `ImportedMedia::Skipped { count: unmatched.len(), files: unmatched.iter().map(|u| u.file_name.clone()).collect() }` if `unmatched` non-empty (preserves current behavior at `transfer.rs:35-43`).

**Call site changes** — all three sites change from a single closure body to a two-step (identify + import) closure body. The `recorded.execute(source, || async { ... })` boundary stays exactly where it is, preserving `ImportRecord` write timing.

- **`app/src/interface/telegram/handler.rs:122-139`**
  - Add `identify_service: IdentifyService` to `ProcessMediaSourcesHandler` (constructed alongside `import_service`).
  - Closure becomes:
    ```rust
    let outcome = handler.recorded_import.execute(import_source, || async {
        let outcome = identify_service.identify(media_files).await?;
        import_service.import_groups(outcome.groups, outcome.unmatched).await
    }).await;
    ```

- **`app/src/application/file_index_import.rs:131-138`** — function signature gains `identify_service: &mut impl IdentifyForImport` (or simpler: take `&mut MediaIdentifyService<M, T>` generic). Per-fingerprint loop body:
  ```rust
  let outcome = recorded.execute(source, || async {
      let mut metadata_lookup = MetadataLookup::default();
      let media_files = metadata_lookup.build_media_files(raw_files.clone(), descriptions);
      let identified = identify_service.identify(media_files).await?;
      importer.import_groups(identified.groups, identified.unmatched).await
  }).await;
  ```
  For behavior-preservation, **keep `MetadataLookup::default()` inline** (current per-fingerprint isolation).
  Update `Console`/`http` wiring that constructs `FileIndexImportService` to also plumb an `IdentifyService` reference.

- **`app/src/interface/cli/handler.rs:47-50`** — add identify call before importer call, mirroring the telegram pattern. Update `CliContext` to expose `identify_service()` alongside `import_service()`.

**`app/src/infrastructure/services.rs`** — update type aliases:
```rust
pub type ImportService    = TransferWorkflow<PanLibraryGateway, FilesystemImportLocalStore>;
pub type IdentifyService  = MediaIdentifyService<TmdbMetadataGateway, TitleExtractorService>;
```

**Test fakes** — update both `FakeImporter` impls (in `application/import_tests.rs:234-245` and inline in `file_index_import.rs::tests`) to the new `import_groups(Vec<Media>, Vec<UnmatchedFile>)` signature. The fakes ignore input — only the type signature changes. Update `TestImportService` in `application/import_tests.rs:20-52` to construct and call a `MediaIdentifyService` alongside, so its `import_from_raw_files` helper still works end-to-end.

**Green criterion**: full test suite passes; split is complete.

### Step 5 — Cleanup

- Remove unused `use` lines (`MetadataCatalog`, `TitleExtractor` imports in transfer modules).
- Run `make fmt`.
- Run `make lint` — Clippy must pass.
- Run `make test` — all tests green.

## Critical files

| Path | Action |
|---|---|
| `app/src/application/import/identify.rs` | **NEW** — MediaIdentifyService + IdentifyOutcome + UnmatchedFile |
| `app/src/application/import/identify/tests.rs` | **NEW** — unit tests (tv hit, movie hit, unmatched, episode-slot None) |
| `app/src/application/import/group.rs` | **DELETE** in step 2 |
| `app/src/application/import/group/tests.rs` | **MOVE** content to identify/tests.rs in step 2 |
| `app/src/application/import/mod.rs` | Update `mod`/`pub use` |
| `app/src/application/import/factory.rs` | Drop generics M,T; drop tmdb/metadata fields |
| `app/src/application/import/transfer.rs` | Adapt to IdentifyOutcome; build ref views from owned Media |
| `app/src/domain/import/inner.rs` | `Media` → owned (no `'a`) |
| `app/src/domain/import/policy.rs` | `insert_*_media` take owned MediaFile |
| `app/src/application/import_ports.rs` | `MediaImporter::import_groups(Vec<Media>, Vec<UnmatchedFile>)` |
| `app/src/infrastructure/services.rs` | `ImportService` alias simplified; add `IdentifyService` alias |
| `app/src/interface/telegram/handler.rs` | Add `identify_service` field; two-step closure |
| `app/src/interface/cli/handler.rs` | Add identify call; CLI context exposes identify |
| `app/src/interface/cli/context.rs` | Construct `IdentifyService` alongside `ImportService` |
| `app/src/application/file_index_import.rs` | Accept identify_service param; two-step closure |
| `app/src/interface/http/console.rs` (FileIndexImport wiring) | Plumb identify service |
| `app/src/application/import_tests.rs` | Update `TestImportService` + `FakeImporter` to new shape |

## Reusable elements already present

- `TmdbLookup` (in `application/import/tmdb_info.rs`) is already `Clone`, `pub(super)`, lifts directly into `MediaIdentifyService`.
- `MetadataLookup` (in `application/import/metadata.rs`) is already used at call sites; only its location-on-TransferWorkflow goes away.
- `domain/import/policy.rs::{insert_movie_media, insert_tv_media, resolve_tv_episode_slot}` already encapsulate grouping rules.
- `FakeMetadataCatalog` / `FakeTitleExtractor` (in `application/import/tmdb_info/tests.rs`) — reused for identify tests.
- `RecordedImportService::execute<F, Fut>` (`application/recorded_import.rs:25-78`) — signature unchanged; closure body changes.
- `ImportedMedia::Skipped { count, files }` — already exists, reused for unmatched aggregation.

## Verification

End-to-end checks before commit/PR:

1. `make fmt` — formatting clean.
2. `make lint` — Clippy passes with `-D warnings`.
3. `make test` — full workspace tests green (this is the behavior-preservation contract).
4. **New tests added in step 1 must all pass** and cover: tv match, movie match, unmatched (TMDB miss), unmatched (episode slot None).
5. **Existing tests must pass without semantic edits** — only signature-mechanical updates allowed. If any existing test requires logic changes, that's a red flag the refactor altered behavior.

No manual app boot required — this is a pure refactor with no user-visible change.

## Landmines

- **`TmdbLookup` is `pub(super)`** to `application::import`. Since identify lives in `application/import/identify.rs`, it can reach it via `super::tmdb_info::TmdbLookup` without changing visibility.
- **`group/tests.rs` imports `resolve_tv_episode_slot` from `application::import::group`** — this is a re-export. After group.rs deletion, repoint to `crate::domain::import::policy::resolve_tv_episode_slot`.
- **Owned conversion in step 3** is the bulk of borrow-checker work. Build small ref-view helpers in transfer.rs to avoid touching every transfer/* submodule.
- **`<M, T>` bounds on `impl<L, M, F, T>` blocks** in transfer/* files exist purely for shared bounds — must prune in step 4.
- **CLI context per-command construction** — IdentifyService is fresh per CLI invocation. Don't try to share across commands.
- **`MetadataLookup` inline in file_index_import** stays inline (per-fingerprint isolation) for behavior preservation. Don't optimize.

## Out of scope (do not do in this PR)

- Subscription concept (that's #121).
- Unifying `ParseService` and `MediaIdentifyService` (they share `TmdbLookup` but have different output shapes; future consolidation only).
- CONTEXT.md changes (parked on `docs/subscription-context-term` for #121).
- ADR for identify/import separation — the merged ADR in #121 (`0002-subscription-as-import-gate.md`) will reference this refactor's motivation.
