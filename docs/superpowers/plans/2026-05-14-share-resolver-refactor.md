# Share Resolver Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将分享解析能力从 `ShareSource` + `ShareCrawler` + `ShareUrl` 的跨层分叉结构收敛到统一 `ShareResolver` 接口和 `infrastructure/share` 实现，同时保持现有 pan123、pan189、pan115、quark、fslink、JSON/CAS 的用户可见行为不变。

**Architecture:** application 层新增统一 `ShareResolver` 端口并消费 `domain::share::RawFile`；`infrastructure/share` 承担 URL 识别、provider 解析流程、通用遍历辅助和文件解析；CLI / Telegram 入口直接调用 resolver 或 share file parser，不再依赖 provider-specific 中间抽象。

**Tech Stack:** Rust workspace, async traits, existing provider clients under `app/src/infrastructure/client/*`, `cargo test`, `cargo fmt`, `cargo clippy`

---

## File Structure

- Create: `app/src/domain/share.rs`
  - 承载 `RawFile`、`Etag`
- Modify: `app/src/domain/mod.rs`
  - 暴露 `share` 模块
- Modify: `app/src/domain/import/inner.rs`
  - 删除 `RawFile` / `Etag` 定义，仅保留 import 专属模型
- Create: `app/src/application/ports/share.rs`
  - 定义统一 `ShareResolver`
- Modify: `app/src/application/mod.rs`
  - 暴露 `ports/share.rs`
- Modify: `app/src/application/ports.rs`
  - 仅保留既有通用端口，不再承载分享解析接口
- Modify: `app/src/application/import/mod.rs`
  - 改为从 `domain::share` re-export `RawFile`，删除 `ShareUrl` / `is_fslink` re-export
- Create: `app/src/infrastructure/share/mod.rs`
  - share 层模块入口
- Create: `app/src/infrastructure/share/resolver.rs`
  - 中心 `ShareResolverService`
- Create: `app/src/infrastructure/share/file_parser.rs`
  - fslink / JSON / CAS 解析
- Create: `app/src/infrastructure/share/pan123.rs`
- Create: `app/src/infrastructure/share/pan189.rs`
- Create: `app/src/infrastructure/share/pan115.rs`
- Create: `app/src/infrastructure/share/quark.rs`
  - 各 provider URL 匹配和解析实现
- Modify: `app/src/infrastructure/mod.rs`
  - 暴露 `share`
- Modify: `app/src/infrastructure/services.rs`
  - 将 `ShareSourceService` 改为 `ShareResolverService`
- Modify: `app/src/interface/cli/handler.rs`
  - 改为直接使用 `Url` + `ShareResolver` / `ShareFileParser`
- Modify: `app/src/interface/telegram/handler.rs`
  - 改为直接使用 `Url` + `ShareResolver` / `ShareFileParser`
- Modify: `app/src/application/import/import_tests.rs`
  - 用 `FakeShareResolver` / `ShareFileParser` 替换 `FakeShareSource` / `ShareCrawler` / `ShareUrl`
- Delete: `app/src/application/share_crawler.rs`
- Delete: `app/src/domain/import/source.rs`
- Delete: `app/src/domain/import/share_collect.rs`
- Delete or shrink: `app/src/domain/import/share_walk.rs`
- Delete: `app/src/infrastructure/import/gateway.rs` 中 `ShareImportGateway` 与 `ShareSource` 实现
- Modify: `app/src/application/import_ports.rs`
  - 删除 `ShareSource` trait

### Task 1: 建立新的领域与应用层边界

**Files:**
- Create: `app/src/domain/share.rs`
- Modify: `app/src/domain/mod.rs`
- Modify: `app/src/domain/import/inner.rs`
- Create: `app/src/application/ports/share.rs`
- Modify: `app/src/application/mod.rs`
- Modify: `app/src/application/import/mod.rs`
- Test: `app/src/domain/share.rs`

- [ ] **Step 1: 写失败测试，锁定 `Etag` 和 `RawFile` 的归属行为**

```rust
#[cfg(test)]
mod tests {
    use super::{Etag, RawFile};

    #[test]
    fn etag_from_str_detects_sha1_and_lowercases() {
        let sha1 = Etag::from("ABCDEF0123456789ABCDEF0123456789ABCDEF01");
        let md5 = Etag::from("ABCDEF0123456789ABCDEF0123456789");

        assert!(matches!(sha1, Etag::Sha1(value) if value == "abcdef0123456789abcdef0123456789abcdef01"));
        assert!(matches!(md5, Etag::Md5(value) if value == "abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn raw_file_keeps_import_relevant_fields() {
        let file = RawFile {
            id: Some(1),
            name: "movie.mkv".into(),
            etag: Etag::from("etag"),
            size: 42,
            path: "/share".into(),
        };

        assert_eq!(file.id, Some(1));
        assert_eq!(file.name, "movie.mkv");
        assert_eq!(file.size, 42);
        assert_eq!(file.path, "/share");
    }
}
```

- [ ] **Step 2: 运行测试，确认新模块尚不存在**

Run: `cargo test domain::share::tests::etag_from_str_detects_sha1_and_lowercases -- --exact`

Expected: FAIL，报 `could not find share in domain` 或测试目标不存在。

- [ ] **Step 3: 实现新的 `domain::share` 与 `application::ports::share`**

```rust
// app/src/domain/share.rs
#[derive(Debug, Clone)]
pub struct RawFile {
    pub id: Option<i64>,
    pub name: String,
    pub etag: Etag,
    pub size: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum Etag {
    Md5(String),
    Sha1(String),
}

impl From<&str> for Etag {
    fn from(s: &str) -> Self {
        if s.len() == 40 {
            Self::Sha1(s.to_lowercase())
        } else {
            Self::Md5(s.to_lowercase())
        }
    }
}

// app/src/application/ports/share.rs
use url::Url;

use crate::{domain::share::RawFile, error::AppResult};

pub trait ShareResolver: Clone {
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>>;
}
```

- [ ] **Step 4: 更新旧引用，移除 `inner.rs` 中的 `RawFile` / `Etag` 定义**

```rust
// app/src/domain/import/inner.rs
use crate::domain::{media::Metadata, share::RawFile};

use super::{MovieDetail, TvDetail};

/// 表示一个媒体文件，包含视频文件和字幕文件
#[derive(Debug)]
pub(crate) struct MediaFile {
    pub metadata: Box<Metadata>,
    pub video: RawFile,
    pub subtitles: Vec<RawFile>,
}
```

- [ ] **Step 5: 运行边界测试并做一次小提交**

Run: `cargo test domain::share::tests -- --nocapture`

Expected: PASS，新的 `domain::share` 测试通过。

```bash
git add app/src/domain/share.rs app/src/domain/mod.rs app/src/domain/import/inner.rs app/src/application/ports/share.rs app/src/application/mod.rs app/src/application/import/mod.rs
git commit -m "refactor: introduce share domain model and resolver port"
```

### Task 2: 提取通用 share 文件解析器

**Files:**
- Create: `app/src/infrastructure/share/file_parser.rs`
- Modify: `app/src/infrastructure/share/mod.rs`
- Modify: `app/src/interface/telegram/handler.rs`
- Modify: `app/src/interface/cli/handler.rs`
- Test: `app/src/infrastructure/share/file_parser.rs`

- [ ] **Step 1: 写失败测试，覆盖 fslink、JSON、base64 JSON 三种输入**

```rust
#[cfg(test)]
mod tests {
    use super::ShareFileParser;

    #[test]
    fn parses_fslink_into_raw_files() {
        let parser = ShareFileParser::default();
        let files = parser
            .raw_files_from_fslink("123FSLinkV2$etag#12#/Movies/movie.mkv")
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "movie.mkv");
        assert_eq!(files[0].path, "/Movies");
    }

    #[test]
    fn parses_object_json_into_raw_files() {
        let parser = ShareFileParser::default();
        let json = br#"{"commonPath":"/TV","files":[{"path":"Season 1/E01.mkv","md5":"abc","size":1}]}"#.to_vec();
        let files = parser.raw_files_from_json(json).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/TV/Season 1");
    }
}
```

- [ ] **Step 2: 运行测试，确认解析器尚未实现**

Run: `cargo test infrastructure::share::file_parser::tests -- --nocapture`

Expected: FAIL，报 `could not find share in infrastructure` 或 `ShareFileParser` 未定义。

- [ ] **Step 3: 从 `domain/import/source.rs` 与 `share_crawler.rs` 搬出解析逻辑**

```rust
#[derive(Clone, Default)]
pub struct ShareFileParser;

impl ShareFileParser {
    pub fn raw_files_from_fslink(&self, fslink: &str) -> AppResult<Vec<RawFile>> {
        let resource = parse_fslink_resource(fslink)?;
        Ok(raw_files_from_resource(&resource))
    }

    pub fn raw_files_from_json(&self, json: Vec<u8>) -> AppResult<Vec<RawFile>> {
        let resource: ResourceJson = parse_files_from_json(json)?;
        Ok(raw_files_from_resource(&resource))
    }
}
```

- [ ] **Step 4: 让 CLI / Telegram 使用 `ShareFileParser`，不再通过 `ShareCrawler` 解析 fslink / JSON**

```rust
// interface/telegram/handler.rs
match source {
    MediaSource::Fslink(fslink) => handler.share_file_parser.raw_files_from_fslink(fslink),
    MediaSource::TgDocument { .. } => handler.share_file_parser.raw_files_from_json(content),
    MediaSource::ShareUrl(url) => handler.share_resolver.raw_files_from_url(&parsed_url).await,
}
```

- [ ] **Step 5: 运行解析器测试并提交**

Run: `cargo test infrastructure::share::file_parser::tests -- --nocapture`

Expected: PASS，fslink / JSON / base64 JSON 解析均通过。

```bash
git add app/src/infrastructure/share/file_parser.rs app/src/infrastructure/share/mod.rs app/src/interface/telegram/handler.rs app/src/interface/cli/handler.rs
git commit -m "refactor: extract share file parser"
```

### Task 3: 建立 provider share service 与中心 resolver

**Files:**
- Create: `app/src/infrastructure/share/pan123.rs`
- Create: `app/src/infrastructure/share/pan189.rs`
- Create: `app/src/infrastructure/share/pan115.rs`
- Create: `app/src/infrastructure/share/quark.rs`
- Create: `app/src/infrastructure/share/resolver.rs`
- Modify: `app/src/infrastructure/mod.rs`
- Modify: `app/src/infrastructure/services.rs`
- Test: `app/src/infrastructure/share/pan189.rs`
- Test: `app/src/infrastructure/share/quark.rs`
- Test: `app/src/infrastructure/share/resolver.rs`

- [ ] **Step 1: 写失败测试，锁定 resolver 路由与 unsupported 行为**

```rust
#[tokio::test]
async fn resolver_returns_none_for_unsupported_url() {
    let resolver = ShareResolverService::new(
        Pan123ShareService::noop(),
        Pan189ShareService::noop(),
        Pan115ShareService::noop(),
        QuarkShareService::noop(),
    );

    let url = url::Url::parse("https://example.com/share/1").unwrap();
    let result = resolver.raw_files_from_url(&url).await.unwrap();

    assert!(result.is_none());
}
```

- [ ] **Step 2: 写失败测试，锁定 pan189 `.cas` 与 quark md5 二阶段语义**

```rust
#[tokio::test]
async fn pan189_expands_cas_when_share_contains_only_cas_files() {
    let service = build_pan189_service_with_cas_fixture();

    let url = url::Url::parse("https://cloud.189.cn/t/abcdef").unwrap();
    let files = service.raw_files_from_url(&url).await.unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "Episode 01.mkv");
}

#[tokio::test]
async fn quark_populates_md5_after_batch_lookup() {
    let service = build_quark_service_fixture();

    let url = url::Url::parse("https://pan.quark.cn/s/share-id?pwd=1234").unwrap();
    let files = service.raw_files_from_url(&url).await.unwrap();

    assert!(matches!(&files[0].etag, crate::domain::share::Etag::Md5(value) if value == "md5-1"));
}
```

- [ ] **Step 3: 把旧 `ShareCrawler` provider-specific 流程按 provider 下沉**

```rust
// app/src/infrastructure/share/pan123.rs
#[derive(Clone)]
pub struct Pan123ShareService {
    client: crate::infrastructure::client::pan123::Client,
}

impl Pan123ShareService {
    pub fn match_url(url: &Url) -> bool { /* 从 source.rs 迁移 */ }

    pub async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
        let (share_key, share_password) = parse_share_parts(url);
        let mut traversal = ShareTraversal::new((0, String::new()));
        while let Some((parent_id, parent_path)) = traversal.next_dir() {
            let files = self.client.list_share_files(share_key, share_password, parent_id).await?;
            traversal.extend(collect_directory_entries(&files, &parent_path));
        }
        Ok(traversal.into_raw_files())
    }
}
```

- [ ] **Step 4: 实现中心 `ShareResolverService`，只做匹配与分发**

```rust
#[derive(Clone)]
pub struct ShareResolverService {
    pan123: Pan123ShareService,
    pan189: Pan189ShareService,
    pan115: Pan115ShareService,
    quark: QuarkShareService,
}

impl crate::application::ports::share::ShareResolver for ShareResolverService {
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>> {
        if Pan123ShareService::match_url(url) {
            self.pan123.raw_files_from_url(url).await.map(Some)
        } else if Pan189ShareService::match_url(url) {
            self.pan189.raw_files_from_url(url).await.map(Some)
        } else if Pan115ShareService::match_url(url) {
            self.pan115.raw_files_from_url(url).await.map(Some)
        } else if QuarkShareService::match_url(url) {
            self.quark.raw_files_from_url(url).await.map(Some)
        } else {
            Ok(None)
        }
    }
}
```

- [ ] **Step 5: 运行 provider 与 resolver 测试并提交**

Run: `cargo test infrastructure::share:: -- --nocapture`

Expected: PASS，resolver 路由测试通过，pan189 `.cas` 与 quark md5 回归测试通过。

```bash
git add app/src/infrastructure/share app/src/infrastructure/mod.rs app/src/infrastructure/services.rs
git commit -m "refactor: add share resolver services"
```

### Task 4: 删除 `ShareSource`，收敛 infrastructure gateway 职责

**Files:**
- Modify: `app/src/application/import_ports.rs`
- Modify: `app/src/infrastructure/import/gateway.rs`
- Modify: `app/src/infrastructure/services.rs`
- Test: `app/src/infrastructure/share/resolver.rs`

- [ ] **Step 1: 写失败测试，确保应用层不再依赖 `ShareSource` trait 名称**

```rust
#[test]
fn share_source_trait_is_removed_from_import_ports() {
    let source = std::fs::read_to_string("app/src/application/import_ports.rs").unwrap();
    assert!(!source.contains("trait ShareSource"));
}
```

- [ ] **Step 2: 运行测试，确认旧 trait 仍存在**

Run: `cargo test application::import::import_tests::share_source_trait_is_removed_from_import_ports -- --exact`

Expected: FAIL，字符串仍包含 `trait ShareSource`。

- [ ] **Step 3: 删除 `ShareSource` trait 与 `ShareImportGateway`，保留底层 client gateway**

```rust
// app/src/application/import_ports.rs
pub trait LibraryGateway: Clone { /* unchanged */ }
pub trait MetadataCatalog: Clone { /* unchanged */ }
pub trait ImportLocalStore: Clone { /* unchanged */ }

// app/src/infrastructure/services.rs
pub type ShareResolverRuntimeService = ShareResolverService;
```

- [ ] **Step 4: 更新调用类型别名和构造链路**

```rust
// app/src/infrastructure/services.rs
use crate::infrastructure::share::resolver::ShareResolverService;

pub type ShareResolverServiceRuntime = ShareResolverService;
```

- [ ] **Step 5: 运行编译检查并提交**

Run: `cargo test infrastructure::share::resolver::tests::resolver_returns_none_for_unsupported_url -- --exact`

Expected: PASS，同时 `ShareSource` 不再被引用。

```bash
git add app/src/application/import_ports.rs app/src/infrastructure/import/gateway.rs app/src/infrastructure/services.rs
git commit -m "refactor: remove share source gateway abstraction"
```

### Task 5: 替换 CLI 与 Telegram 调用入口

**Files:**
- Modify: `app/src/interface/cli/handler.rs`
- Modify: `app/src/interface/telegram/handler.rs`
- Modify: `app/src/interface/cli/server.rs`
- Modify: `app/src/infrastructure/services.rs`
- Test: `app/src/interface/cli/handler.rs`

- [ ] **Step 1: 写失败测试，锁定 unsupported share URL 文案和入口行为**

```rust
#[test]
fn parse_share_url_rejects_unsupported_provider() {
    let err = parse_share_url("https://example.com/s/test").unwrap_err();

    assert!(matches!(err, crate::error::AppError::InvalidParameter(_)));
    assert!(err.to_string().contains("unsupported share url"));
}
```

- [ ] **Step 2: 运行测试，确认旧入口仍通过 `ShareUrl::from`**

Run: `cargo test interface::cli::handler::tests::parse_share_url_rejects_unsupported_provider -- --exact`

Expected: PASS 但实现仍依赖旧结构；接下来改为依赖 resolver，测试需保持不变。

- [ ] **Step 3: 让 CLI 直接使用 `ShareResolver` 返回 `Ok(None)` 判 unsupported**

```rust
let parsed_url = Url::parse(raw_url)
    .map_err(|err| AppError::InvalidParameter(format!("invalid share url '{raw_url}': {err}")))?;

let raw_files = match share_resolver.raw_files_from_url(&parsed_url).await? {
    Some(files) => files,
    None => {
        return Err(AppError::InvalidParameter(format!(
            "unsupported share url '{raw_url}', expected pan123, pan189, pan115, or quark share link"
        )));
    }
};
```

- [ ] **Step 4: 让 Telegram 入口改用 `share_resolver` + `share_file_parser`**

```rust
pub struct ProcessMediaSourcesHandler {
    pub share_resolver: ShareResolverService,
    pub share_file_parser: ShareFileParser,
    // ...
}
```

- [ ] **Step 5: 运行入口测试并提交**

Run: `cargo test interface::cli::handler::tests -- --nocapture`

Expected: PASS，unsupported / invalid URL 文案不变，CLI 编译通过。

```bash
git add app/src/interface/cli/handler.rs app/src/interface/telegram/handler.rs app/src/interface/cli/server.rs app/src/infrastructure/services.rs
git commit -m "refactor: switch interfaces to share resolver"
```

### Task 6: 迁移 import 测试到新抽象

**Files:**
- Modify: `app/src/application/import/import_tests.rs`
- Test: `app/src/application/import/import_tests.rs`

- [ ] **Step 1: 写失败测试，先把现有 `TestImportService` 改成 resolver 依赖**

```rust
pub(crate) struct TestImportService<L, R, M, F> {
    pub resolver: R,
    pub transfer: TransferWorkflow<L, M, F>,
    pub metadata_lookup: MetadataLookup,
}

impl<L, R, M, F> TestImportService<L, R, M, F>
where
    L: LibraryGateway,
    R: ShareResolver,
    M: MetadataCatalog,
    F: ImportLocalStore,
{
    pub async fn import_from_share_url(&mut self, url: &Url) -> AppResult<Vec<ImportedMedia>> {
        let raw_files = self
            .resolver
            .raw_files_from_url(url)
            .await?
            .ok_or_else(|| AppError::InvalidParameter(format!("unsupported share url: {url}")))?;
        let media_files = self.metadata_lookup.build_media_files(raw_files);
        self.transfer.transfer_media_files(&media_files).await
    }
}
```

- [ ] **Step 2: 运行测试，确认旧 fake 仍然实现的是 `ShareSource`**

Run: `cargo test application::import::import_tests -- --nocapture`

Expected: FAIL，泛型约束或 fake 实现需要从 `ShareSource` 改为 `ShareResolver`。

- [ ] **Step 3: 用 `FakeShareResolver` 替换 `FakeShareSource`，把 fslink / JSON 测试改为 `ShareFileParser`**

```rust
#[derive(Clone, Default)]
struct FakeShareResolver {
    raw_files_by_url: Arc<Mutex<HashMap<String, Vec<RawFile>>>>,
}

impl crate::application::ports::share::ShareResolver for FakeShareResolver {
    async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>> {
        Ok(self.raw_files_by_url.lock().unwrap().get(url.as_str()).cloned())
    }
}
```

- [ ] **Step 4: 补一个 unsupported URL 测试，覆盖新的 `Option<Vec<RawFile>>` 语义**

```rust
#[tokio::test]
async fn import_from_share_url_rejects_unsupported_provider() {
    let mut service = build_test_import_service_with_empty_resolver();
    let url = Url::parse("https://example.com/share").unwrap();

    let err = service.import_from_share_url(&url).await.unwrap_err();

    assert!(err.to_string().contains("unsupported share url"));
}
```

- [ ] **Step 5: 运行 import 测试并提交**

Run: `cargo test application::import::import_tests -- --nocapture`

Expected: PASS，application 层测试不再依赖 provider-specific gateway 方法。

```bash
git add app/src/application/import/import_tests.rs
git commit -m "test: migrate import tests to share resolver"
```

### Task 7: 删除旧结构并完成全量验证

**Files:**
- Delete: `app/src/application/share_crawler.rs`
- Delete: `app/src/domain/import/source.rs`
- Delete: `app/src/domain/import/share_collect.rs`
- Modify: `app/src/domain/import/share_walk.rs`
- Modify: `app/src/domain/import/mod.rs`
- Modify: `app/src/application/mod.rs`
- Modify: any remaining imports found by ripgrep
- Test: workspace

- [ ] **Step 1: 写失败检查，确保旧符号不再存在于源码引用中**

```bash
rg -n "ShareCrawler|ShareSource|ShareUrl::from|domain::import::source|share_collect" app/src
```

Expected: 先有匹配结果，作为删除前基线。

- [ ] **Step 2: 删除旧文件和旧模块暴露，保留必要的通用遍历辅助**

```rust
// app/src/domain/import/mod.rs
mod inner;
pub(crate) mod model;

pub(crate) use inner::{Media, MediaFile, TransferEpisodeArgs};

// app/src/application/mod.rs
pub mod delete_media;
pub mod emby_proxy;
pub mod file_index;
pub mod import;
pub mod import_ports;
pub mod manage_keywords;
pub mod notify;
pub mod ports;
pub mod resolve_download_url;
pub mod sync_strm;
```

- [ ] **Step 3: 运行源码扫描，确认旧符号全部消失**

Run: `rg -n "ShareCrawler|ShareSource|ShareUrl::from|domain::import::source|share_collect" app/src`

Expected: 无输出。

- [ ] **Step 4: 运行格式化、定向测试和全量 lint**

Run: `cargo fmt --all`

Expected: PASS，无格式化错误。

Run: `cargo test`

Expected: PASS，所有现有测试通过，重点验证 pan189 `.cas`、quark md5、CLI / Telegram share 入口、import tests。

Run: `cargo clippy -- -D warnings`

Expected: PASS，无 warnings。

- [ ] **Step 5: 完成收尾提交**

```bash
git add app/src docs/superpowers/plans/2026-05-14-share-resolver-refactor.md
git commit -m "refactor: centralize share resolver flow"
```

## Self-Review

- Spec coverage:
  - 统一 `ShareResolver` 接口：Task 1
  - `RawFile` 迁移到 `domain/share.rs`：Task 1
  - fslink / JSON 独立解析：Task 2
  - `infrastructure/share/*` provider 实现与中心 resolver：Task 3
  - 删除 `ShareSource` / gateway 聚合层：Task 4
  - CLI / Telegram 替换：Task 5
  - 测试迁移：Task 6
  - 删除旧结构与全量验证：Task 7
- Placeholder scan:
  - 未保留 `TODO` / `TBD` / “按需处理” 类占位描述。
- Type consistency:
  - 统一使用 `crate::domain::share::RawFile`
  - 统一使用 `crate::application::ports::share::ShareResolver`
  - unsupported 语义统一为 `AppResult<Option<Vec<RawFile>>>`
