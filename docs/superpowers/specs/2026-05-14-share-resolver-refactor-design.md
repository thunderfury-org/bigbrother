# Share Resolver 重构 Spec

## 背景

当前“网盘分享解析”能力分散在多层抽象中，新增一个新的网盘分享源时，通常需要同时修改多处代码：

- `domain/import/source.rs`
  - `ShareUrl` 枚举
  - URL 识别和参数提取函数
- `application/import_ports.rs`
  - `ShareSource` trait
- `application/share_crawler.rs`
  - 按网盘分叉的抓取流程
- `domain/import/share_collect.rs`
  - 按网盘拆分目录项
- `infrastructure/import/gateway.rs`
  - `ShareSource` 的具体实现
- `interface/cli/handler.rs`
  - 分享 URL 校验
- `interface/telegram/handler.rs`
  - 分享 URL 校验和抓取入口
- `application/import/import_tests.rs`
  - `FakeShareSource`

这导致“新增一个网盘分享处理”会跨 `domain` / `application` / `infrastructure` / `interface` 多层扩散，结构边界不清晰。

## 现状问题

### 1. 分享源抽象按 provider 展开，扩展面过大

当前 `ShareSource` trait 直接暴露各网盘的底层能力，例如：

- `list_pan123_share_files`
- `get_pan189_share_info`
- `list_pan189_share_files`
- `download_pan189_share_file`
- `list_pan115_share_files`
- `get_quark_share_info`
- `list_quark_share_files`
- `batch_get_quark_file_md5s`

这意味着 application 层显式知道每个 provider 的处理差异。

### 2. 业务层承载了 provider 特有流程

当前 `ShareCrawler` 中存在大量 provider 特有流程：

- pan123: 直接 BFS 目录遍历
- pan189: 分享信息获取、目录遍历、`.cas` 文件识别和展开
- pan115: 目录遍历
- quark: 先 BFS 列目录，再二阶段批量补 md5

这些流程应属于分享源实现细节，不应暴露在 application 业务层。

### 3. URL 识别与解析逻辑分裂

当前 URL 识别在 `domain/import/source.rs` 中，真正的分享解析流程在 `application/share_crawler.rs` 中，导致规则与实现分开维护。

### 4. `client` 与“分享解析编排”边界不够清晰

当前 `infrastructure/import/gateway.rs` 充当了多网盘聚合适配层，但其接口形态仍然受旧的 `ShareSource` 设计影响，无法体现“client 只做底层 API，share 层负责完整分享解析流程”的边界。

## 重构目标

### 核心目标

- 将“分享解析逻辑”集中到 `infrastructure/share`
- application 层只依赖统一分享解析接口
- 删除旧的 `ShareSource` / `ShareUrl` / `ShareCrawler` 分叉设计
- `client` 模块只保留底层 API 能力，不承载分享解析编排
- 新增 provider 时，将改动面收敛到：
  - 新增一个 provider share service
  - 在中心 resolver 中增加一条路由
  - 如有需要，补充对应 client API

### 非目标

- 本次不重构导入、TMDB 匹配、传输、索引等后续业务流程
- 本次不强行统一所有 provider 的内部实现模式
- 本次不追求“新增 provider 时完全不改中心代码”

## 目标设计

## 一、统一应用层接口

在 application 层定义统一分享解析接口：

```rust
pub trait ShareResolver: Clone {
    async fn raw_files_from_url(&self, url: &url::Url) -> AppResult<Option<Vec<RawFile>>>;
    // url 类型也可以是 &str 这个要看下具体用哪个更好
}
```

语义：

- 输入原始分享 URL
- 输出标准化的 `Vec<RawFile>`
- 不再暴露 provider-specific 的分享能力接口

说明：

- application 层不再感知 pan123 / pan189 / pan115 / quark 的差异
- application 层也不再持有 `ShareUrl` 这类中间抽象

## 二、分享解析实现集中到 `infrastructure/share`

新增模块：

- `app/src/infrastructure/share/mod.rs`
- `app/src/infrastructure/share/resolver.rs`
- `app/src/infrastructure/share/pan123.rs`
- `app/src/infrastructure/share/pan189.rs`
- `app/src/infrastructure/share/pan115.rs`
- `app/src/infrastructure/share/quark.rs`

职责：

- `resolver.rs`
  - 实现统一 `ShareResolver`
  - 集中进行 URL 路由
- 各 provider 模块
  - 定义 URL 匹配函数
  - 完整实现该 provider 的分享解析流程
  - 输出统一 `Vec<RawFile>`

## 三、中心 Resolver 采用集中式路由

`ShareResolverService` 统一入口使用 `if / else if` 路由：

```rust
pub struct ShareResolverService {
    pan123: Pan123ShareService,
    pan189: Pan189ShareService,
    pan115: Pan115ShareService,
    quark: QuarkShareService,
}
```

伪代码：

```rust
async fn raw_files_from_url(&self, url: &Url) -> AppResult<Vec<RawFile>> {
    if pan123::match_url(url) {
        self.pan123.raw_files_from_url(url).await
    } else if pan189::match_url(url) {
        self.pan189.raw_files_from_url(url).await
    } else if pan115::match_url(url) {
        self.pan115.raw_files_from_url(url).await
    } else if quark::match_url(url) {
        self.quark.raw_files_from_url(url).await
    } else {
        Err(AppError::InvalidParameter(format!("unsupported share url: {url}")))
    }
}
```

约束：

- 路由只负责匹配和分发
- 不能在中心 resolver 中承载 provider 特有流程
- 特殊逻辑必须留在各 provider share service 内

## 四、URL 匹配规则跟随 provider 放置

每个 provider 模块自行定义 URL 匹配函数，例如：

```rust
pub fn match_url(url: &Url) -> bool
```

设计原因：

- 保持“规则”和“解析实现”在同一处
- 避免重新出现“中心统一识别，provider 单独解析”的分裂
- 便于后续调整各 provider 的 host/path 兼容规则

## 五、provider 内承载完整分享解析流程

### pan123

职责：

- 解析分享参数
- BFS 遍历目录
- 输出 `RawFile`

### pan189

职责：

- 解析分享码
- 获取分享信息
- BFS 遍历目录
- 检测 `.cas` 文件
- 如果分享内容仅包含 `.cas`，下载并展开 `.cas` 内容
- 输出 `RawFile`

### pan115

职责：

- 解析分享码 / 提取码
- BFS 遍历目录
- 输出 `RawFile`

### quark

职责：

- 解析分享 ID / 密码
- 获取 `stoken`
- BFS 遍历目录
- 收集文件 `fid/share_fid_token`
- 二阶段批量获取 md5
- 输出 `RawFile`

## 六、共享逻辑的保留边界

保留：

- `share_walk.rs`
  - 作为真正通用的目录遍历辅助

迁移：

- `share_collect.rs`
  - 当前本质上是按 provider 分叉的转换逻辑
  - 应拆回各 provider 模块内部

原则：

- 真正 provider 无关的工具可以保留公共模块
- provider-specific 的目录项收集和转换逻辑应跟 provider 实现放在一起

## 七、`client` 模块职责收敛

`infrastructure/client/*` 仅保留底层 API 调用能力，例如：

- 发起 HTTP 请求
- 请求参数拼装
- 响应解析
- provider API 的错误转换

不再承载：

- 分享 URL 识别
- 分享解析编排
- BFS 遍历
- `.cas` 展开
- 二阶段 md5 收集

这些能力统一放到 `infrastructure/share/*`。

## 模块迁移建议

### 保留

- `app/src/domain/import/inner.rs`
- `app/src/domain/import/share_walk.rs`
- `app/src/domain/import/source.rs` 中与 fslink / JSON 解析相关的内容
- `app/src/infrastructure/client/*`
- `app/src/application/import/*`

### 删除或迁移

- `app/src/application/share_crawler.rs`
  - 删除，能力迁移到 `infrastructure/share/*`
- `app/src/application/import_ports.rs` 中的 `ShareSource`
  - 删除
- `app/src/domain/import/source.rs` 中的 `ShareUrl`
  - 删除
- `app/src/domain/import/source.rs` 中的 provider-specific URL 解析函数
  - 迁移到各 provider share service
- `app/src/domain/import/share_collect.rs`
  - 删除，按 provider 下沉
- `app/src/infrastructure/import/gateway.rs` 中的 `ShareSource` 实现
  - 删除

## 受影响入口

### CLI

当前：

- `interface/cli/handler.rs`
  - `parse_share_url` 使用 `ShareUrl::from`
  - `run_import_share_url` 使用 `ShareCrawler`

目标：

- 直接构造 `Url`
- 使用新的 `ShareResolver`
- unsupported 校验由 `ShareResolver` 统一负责，或由 resolver 暴露统一校验能力

### Telegram

当前：

- `interface/telegram/handler.rs`
  - 使用 `ShareUrl::from`
  - 使用 `ShareCrawler`

目标：

- 直接将 `Url` 传给新的 `ShareResolver`
- 分享处理错误统一由 resolver 返回

### Runtime wiring

当前：

- `infrastructure/services.rs`
  - `ShareSourceService = ShareImportGateway`

目标：

- 改为 `ShareResolverService`
- 更新 CLI / Telegram 构造链路

## 测试调整

### 当前测试形态

`application/import/import_tests.rs` 当前通过：

- `FakeShareSource`
- `ShareCrawler`
- `ShareUrl::from`

来驱动分享导入测试。

### 目标测试形态

按职责拆开：

1. application/import 相关测试
   - 不再依赖 `ShareCrawler`
   - 应直接依赖统一 `ShareResolver` 或更上层已经拿到的 `RawFile`

2. `infrastructure/share/*` provider 测试
   - 各 provider share service 独立测试 URL 识别和分享解析流程

3. resolver 路由测试
   - 测试不同 URL 正确分发到对应 provider
   - 测试 unsupported URL 错误

### 测试原则

- provider-specific 逻辑测试应下沉到 `infrastructure/share/*`
- application 层测试不再感知 provider-specific 的底层 API

## 推荐迁移步骤

### Step 1

在 `application/import_ports.rs` 中引入新的 `ShareResolver` trait。

### Step 2

新建 `infrastructure/share` 模块：

- `resolver.rs`
- `pan123.rs`
- `pan189.rs`
- `pan115.rs`
- `quark.rs`

先迁移现有 `ShareCrawler` 中各 provider 的核心逻辑，保证行为不变。

### Step 3

将 `domain/import/source.rs` 中的 provider-specific URL 识别与参数提取迁移到各 provider 模块。

### Step 4

将 `share_collect.rs` 中的 provider-specific 逻辑迁移到各 provider 模块。

### Step 5

在 `infrastructure/share/resolver.rs` 中实现统一路由。

### Step 6

替换调用方：

- `interface/cli/handler.rs`
- `interface/telegram/handler.rs`
- `interface/cli/server.rs`
- `infrastructure/services.rs`

### Step 7

删除旧结构：

- `ShareSource`
- `ShareCrawler`
- `ShareUrl`
- `ShareSource` 相关 gateway 实现

### Step 8

补齐或迁移测试，确认 provider 行为与旧实现一致。

## 风险与注意事项

### 1. 行为回归风险

`pan189` 的 `.cas` 展开和 `quark` 的二阶段 md5 获取是当前最容易回归的行为，迁移时需要优先保留现有语义。

### 2. 错误文案变化风险

当前 CLI / Telegram 对“unsupported share url”和各类 provider 失败有现成文案，迁移时要避免用户可见行为无意变化过大。

### 3. 测试迁移成本

`application/import/import_tests.rs` 当前对旧抽象耦合较深，迁移时可能需要分拆测试职责，而不是简单机械替换 fake。

### 4. 中心 resolver 膨胀风险

虽然接受 `if / else if` 路由，但必须严格限制 resolver 只做匹配和分发，不能重新吸纳 provider-specific 流程。

## 待确认问题

以下问题尚未完全定稿，需要在实现前确认。

### 1. `ShareResolver` trait 的放置位置

候选：

- 继续放在 `app/src/application/import_ports.rs`
- 新建更贴切的端口文件，例如 `app/src/application/share_ports.rs`

当前建议：

- 如果本次只做分享解析链路重构，可先放在 `import_ports.rs`
- 如果希望 application 边界更清晰，建议拆出 `share_ports.rs`

### 2. `raw_files_from_json` / `raw_files_from_fslink` 的归属

当前这些能力在 `ShareCrawler` 中：

- `raw_files_from_fslink`
- `raw_files_from_json`

待确认它们未来属于：

- A. 继续并入 `ShareResolver`
- B. 单独拆成 `ShareSourceParser` / `ResourceParser`
- C. 先留在一个新的通用解析 service 中，由 CLI / Telegram 继续调用

当前倾向：

- `fslink/json` 不属于“分享 URL 解析”
- 更适合作为独立解析能力保留，不强行并入 `ShareResolver`

### 3. 旧 `domain/import/source.rs` 的保留范围

待确认：

- 是否仅保留 fslink / JSON 解析相关内容
- 是否顺手更名，避免 `source.rs` 继续承载“分享源”和“资源文件格式”两类职责

当前倾向：

- provider-specific URL 内容迁走
- fslink / JSON 解析保留，但可以考虑后续拆分命名

### 4. share 模块的数据模型是否要立即本地化

当前 provider 逻辑复用了多个 domain/import 模型：

- `Pan189ShareInfo`
- `Pan189Folder`
- `Pan189File`
- `Pan115FileEntry`
- `QuarkShareInfo`
- `QuarkFolder`
- `QuarkFile`

待确认：

- A. 本次先继续复用这些模型，降低重构风险
- B. 顺手把这些 provider-specific 模型迁到 `infrastructure/share` 或 `infrastructure/client`

当前倾向：

- 先选 A，避免本轮改造面继续扩大

### 5. unsupported URL 的前置校验是否仍需保留

当前 CLI 中 `parse_share_url` 会在真正抓取前先校验是否为支持的分享 URL。

待确认：

- A. 继续保留单独的前置校验函数
- B. 直接调用 `ShareResolver::raw_files_from_url`，由 resolver 返回 unsupported 错误

当前倾向：

- 选 B，更统一，避免重复一套识别逻辑

## 建议结论

本次重构建议以“分享解析链路中心化”为唯一主目标，先完成以下结构收敛：

- application 只保留统一 `ShareResolver`
- infrastructure/share 集中承载 provider-specific 分享解析
- client 保持纯底层 API 适配器
- 删除 `ShareSource` / `ShareCrawler` / `ShareUrl`

在此基础上，后续如果再新增网盘分享源，改动将主要局限在 share 层，而不会再跨多个架构层级扩散。
