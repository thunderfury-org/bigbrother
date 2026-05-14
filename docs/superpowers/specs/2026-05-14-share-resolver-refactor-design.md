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

### 5. `RawFile` 归属不合理

当前 `RawFile` 位于 `domain/import/inner.rs`，但它表达的是分享来源解析后的原始文件模型，不是 import 专属概念。

目标归属：

- 将 `RawFile` 迁移到 `app/src/domain/share.rs`
- `domain` 保持对 `RawFile` 的所有权
- `infrastructure/share` 负责产出 `RawFile`
- `application` 与 `domain/import` 作为消费者使用 `RawFile`

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
}
```

语义：

- 输入原始分享 URL
- 输出标准化的 `Option<Vec<RawFile>>`
- 不再暴露 provider-specific 的分享能力接口

返回值约定：

- `Ok(Some(files))`
  - 该 resolver 识别并成功解析了分享 URL
- `Ok(None)`
  - 输入不是支持的分享链接
- `Err(e)`
  - 已识别为支持的分享链接，但解析过程中失败

说明：

- application 层不再感知 pan123 / pan189 / pan115 / quark 的差异
- application 层也不再持有 `ShareUrl` 这类中间抽象
- `ShareResolver` 放在 `app/src/application/ports/share.rs`

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
- `ShareFileParser`
  - 独立承载 fslink / JSON 到 `Vec<RawFile>` 的解析

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
async fn raw_files_from_url(&self, url: &Url) -> AppResult<Option<Vec<RawFile>>> {
    if pan123::match_url(url) {
        self.pan123.raw_files_from_url(url).await
    } else if pan189::match_url(url) {
        self.pan189.raw_files_from_url(url).await
    } else if pan115::match_url(url) {
        self.pan115.raw_files_from_url(url).await
    } else if quark::match_url(url) {
        self.quark.raw_files_from_url(url).await
    } else {
        Ok(None)
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

- `app/src/infrastructure/client/*`
- `app/src/application/import/*`
- `app/src/domain/share.rs`
  - 承载 `RawFile` 等分享原始文件模型

### 删除或迁移

- `app/src/domain/import/inner.rs`
  - 删除或收缩，`RawFile` 迁移到 `app/src/domain/share.rs`
- `app/src/domain/import/share_walk.rs`
  - 迁移到 `infrastructure/share/*` 相关位置
- `app/src/domain/import/source.rs` 中与 fslink / JSON 解析相关的内容
  - 迁移到独立的 `ShareFileParser`
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
- unsupported 校验由 `ShareResolver` 返回 `Ok(None)` 统一处理
- fslink / JSON 继续通过独立的 `ShareFileParser` 处理

### Telegram

当前：

- `interface/telegram/handler.rs`
  - 使用 `ShareUrl::from`
  - 使用 `ShareCrawler`

目标：

- 直接将 `Url` 传给新的 `ShareResolver`
- unsupported 分享链接由 resolver 返回 `Ok(None)`
- 分享处理错误统一由 resolver 返回
- fslink / JSON 继续通过独立的 `ShareFileParser` 处理

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
   - 测试 unsupported URL 返回 `Ok(None)`

### 测试原则

- provider-specific 逻辑测试应下沉到 `infrastructure/share/*`
- application 层测试不再感知 provider-specific 的底层 API

## 推荐迁移步骤

### Step 1

在 `application/ports/share.rs` 中引入新的 `ShareResolver` trait，并在 `domain/share.rs` 中安置 `RawFile`。

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

将 `share_collect.rs` 中的 provider-specific 逻辑迁移到各 provider 模块，并将 fslink / JSON 解析迁移到独立的 `ShareFileParser`。

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

## 建议结论

本次重构建议以“分享解析链路中心化”为唯一主目标，先完成以下结构收敛：

- application 只保留统一 `ShareResolver`
- infrastructure/share 集中承载 provider-specific 分享解析
- client 保持纯底层 API 适配器
- fslink / JSON 解析独立为 `ShareFileParser`
- `RawFile` 迁移到 `domain/share.rs`
- 删除 `ShareSource` / `ShareCrawler` / `ShareUrl`

在此基础上，后续如果再新增网盘分享源，改动将主要局限在 share 层，而不会再跨多个架构层级扩散。
