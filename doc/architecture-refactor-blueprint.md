# BigBrother 架构重构蓝图（按当前代码重梳）

> 更新时间：2026-04-08  
> 依据代码：`app/src/*`、`migration/src/*`

## 1. 目标

这份文档不再描述一个抽象的“理想架构”，而是基于当前代码回答三个问题：

- 现在系统实际上是怎么分层和运行的
- 哪些重构已经落地，哪些还只是半完成状态
- 下一步应该沿着什么边界继续收口

当前仓库已经明显从“全局 `AppState` 驱动”走向了“bootstrap + application service + adapter”的结构，但还没有完全进入严格分层。

---

## 2. 当前代码里的真实结构

当前 `app` crate 可以概括为 6 个部分：

```text
main
  -> bootstrap
    -> interface
      -> application
        -> domain
        -> application ports
      -> infrastructure adapters
    -> runtime workers
migration
```

对应代码位置：

- 入口：`app/src/main.rs`
- 运行时组装：`app/src/bootstrap/mod.rs`、`app/src/bootstrap/app.rs`
- 应用层：`app/src/application/*`
- 领域层：`app/src/domain/*`
- 接口层：`app/src/interface/*`
- 基础设施层：`app/src/infrastructure/*`
- 仍待继续收口的旧业务核心：`app/src/library/*`

这里最关键的现实是：**四层目录已经落地，但 `library/import` 仍然是一个尚未完全拆开的核心业务区。**

---

## 3. 运行时启动链路

### 3.1 入口

`app/src/main.rs:1` 当前只有一个 CLI 子命令入口：`server`。

执行流程：

1. 解析 CLI 参数
2. 调用 `AppContext::new(data_dir)`
3. 通过 `AppRuntime::from_app(app)` 组装运行时
4. `AppRuntime::run()` 并行启动：
   - HTTP server
   - Telegram bot dispatcher
   - event delivery worker
   - cache cleanup loop

### 3.2 Bootstrap 的实际职责

`app/src/bootstrap/app.rs:1` 的 `AppContext` 负责创建底层运行时输入：

- `DatabaseConnection`
- `teloxide::Bot`
- `Cache`
- `EventBus`
- 外部 client 集合（`pan115` / `pan123` / `pan189` / `tmdb`）
- import/sync 所需配置

`app/src/bootstrap/mod.rs:1` 的 `AppRuntime::from_app` 再把这些输入组装成具体 service：

- `ManageKeywordsService<SeaOrmKeywordRepository>`
- `ImportMediaService<ImportGateway>`
- `PublishTelegramMessageService<EventBusPublisher>`
- `SyncStrmService<Pan123LibraryRemote, TokioFileStore>`
- `ResolveDownloadUrlService<StringCacheStore, Pan123LibraryRemote>`

这说明当前已经不是“业务函数直接吃 `AppState`”，而是：

- bootstrap 仍持有宽依赖
- interface 层拿到的是已组装好的 service
- application 层大多只依赖 trait 或明确的 adapter

这是当前架构最重要的进步。

---

## 4. 分层现状

### 4.1 Domain 层：已经比较清晰

代码位置：`app/src/domain/*`

当前真正符合 domain 特征的模块主要有两块：

#### `domain/media`

代码位置：

- `app/src/domain/media/mod.rs`
- `app/src/domain/media/parser.rs`
- `app/src/domain/media/normalize.rs`

职责：

- 文件名解析
- 标题/语言/编码/HDR/分辨率等归一化
- 视频/字幕类型识别
- 季/集/年份/TMDB id 提取

特点：

- 规则密集
- 基本不依赖基础设施
- 有较多测试样本
- 已经是稳定的纯业务资产

#### `domain/library`

代码位置：

- `app/src/domain/library/path_mapping.rs`
- `app/src/domain/library/sync_plan.rs`

职责：

- 远端路径 -> 本地路径映射
- `.strm` 路径推导
- 根据远端快照和本地快照生成同步计划
- 识别 stale files / stale dirs

特点：

- 纯函数比例高
- 逻辑和 IO 解耦较好
- 已经支撑 `SyncStrmService`

#### 现状判断

`domain` 层目前已经不是蓝图里的目标态，而是**已经部分落地**。  
其中 `media` 和 `sync_plan` 是当前代码中最接近“可长期稳定复用”的内核。

---

### 4.2 Application 层：主干已经成型

代码位置：`app/src/application/*`

当前 application 层主要包含 5 组 service。

#### `SyncStrmService`

文件：`app/src/application/sync_strm.rs`

依赖：

- `LibraryRemote`
- `FileStore`
- `domain::library::{SyncPathMapper, build_sync_plan}`
- `domain::media::Metadata`

职责：

- 找到远端根目录 id
- 遍历远端目录形成快照
- 遍历本地目录形成快照
- 调用 domain 生成同步计划
- 执行 `.strm` 写入、字幕下载、脏文件清理

结论：

- 这是当前分层最清晰的用例服务之一
- 编排与规则已基本分离
- 已具备 fake 依赖测试能力

#### `ManageKeywordsService`

文件：`app/src/application/manage_keywords.rs`

依赖：

- `KeywordRepository`

职责：

- 列表查询
- keyword trim / empty 校验
- 添加、删除关键字

结论：

- 这是一个典型的小型 use case service
- 结构已经符合目标态

#### `ResolveDownloadUrlService`

文件：`app/src/application/resolve_download_url.rs`

依赖：

- `DownloadUrlCache`
- `DownloadUrlSource`

职责：

- 优先查缓存
- 缓存未命中时获取下载地址
- 将 source error 映射为 redirect / unauthorized / not found

结论：

- 这是 HTTP 下载跳转用例的标准 application service
- 当前设计简洁、边界清楚

#### `ImportMediaService`

文件：`app/src/application/import_media.rs`

依赖：

- `ImportMediaGateway`

职责：

- 对外暴露 `import_from_share_url` / `import_from_fslink` / `import_from_json`
- 自身几乎不承载业务规则，主要作为 application facade

结论：

- 应用层入口已经抽出
- 但真实导入逻辑仍在 `library/import`，应用层还比较薄

#### `notify`

文件：`app/src/application/notify.rs`

包含：

- `PublishTelegramMessageService`
- `DeliverTelegramMessageService`

职责：

- 发送 Telegram 消息事件
- 消费事件并调用发送器

结论：

- 用例编排思路是对的
- 但这里有一个当前代码里的分层泄漏：`application::notify` 直接依赖了 `infrastructure::event::SendTelegramMessage`
- 这意味着消息载荷模型还没有彻底留在 application/domain 一侧

---

### 4.3 Application Ports：已经有第一批稳定端口

文件：`app/src/application/ports.rs`

当前已经存在的端口包括：

- `KeywordRepository`
- `DownloadUrlCache`
- `DownloadUrlSource`
- `LibraryRemote`
- `FileStore`

这些端口有两个明显特征：

1. 是按“用例需要的能力”切的，而不是按技术框架切的
2. 目前重点覆盖了关键字管理、下载地址解析、strm 同步三个方向

这部分已经是当前代码中最值得继续沿用的设计。

但也要注意：**导入链路并没有纳入 `application/ports.rs` 这一统一端口面。**  
`ImportMediaService` 目前通过单独的 `ImportMediaGateway` 挂接，而具体能力接口仍聚集在 `library/import/remote.rs` 里的 `ImportClient`。

---

### 4.4 Interface 层：已从“直接写业务”收缩为“service adapter”

代码位置：`app/src/interface/*`

#### HTTP

文件：

- `app/src/interface/http/mod.rs`
- `app/src/interface/http/media.rs`

当前媒体下载接口的职责比较清楚：

- 读取 path / query 参数
- 校验 `file_id`
- 调用 `ResolveDownloadUrlService`
- 映射为 HTTP 响应

`interface/http/media.rs:1` 里仍然显式使用了：

- `StringCacheStore`
- `Pan123LibraryRemote`

但这部分组装发生在 router context 层，而不是 handler 内部，因此问题不大。

#### Telegram

文件：

- `app/src/interface/telegram/mod.rs`
- `app/src/interface/telegram/cmd.rs`
- `app/src/interface/telegram/msg.rs`
- `app/src/interface/telegram/delivery.rs`
- `app/src/interface/telegram/format.rs`

当前 Telegram 层的职责分为三类：

1. bot runtime 和 handler 注册
2. command 到 application service 的调用
3. message 文本/URL/文档提取与结果格式化

已经完成的收口：

- `/list_keywords`、`/add_keyword`、`/delete_keyword` 走 `ManageKeywordsService`
- `/sync_strm` 走 `SyncStrmService`
- 通知发送走 `PublishTelegramMessageService`
- event 消费后的真正发送走 `DeliverTelegramMessageService`

仍然较厚的部分：

- `interface/telegram/msg.rs` 还承担了较多输入提取和导入触发逻辑
- 它虽然没有直接访问数据库，但仍然包含较复杂的消息解析流程
- 这块更像“interface-specific orchestration”，不算严重越层，但复杂度已经不低

#### CLI

文件：`app/src/interface/cli/mod.rs`

当前 CLI 很薄，只负责解析 `server --data-dir`。

---

### 4.5 Infrastructure 层：接入实现已基本归位

代码位置：`app/src/infrastructure/*`

#### 典型 adapter

- `app/src/infrastructure/repo/keyword.rs`  
  `SeaOrmKeywordRepository` 实现 `KeywordRepository`
- `app/src/infrastructure/cache/string_store.rs`  
  `StringCacheStore` 实现 `DownloadUrlCache`
- `app/src/infrastructure/client/library_remote.rs`  
  `Pan123LibraryRemote` 实现 `LibraryRemote` 与 `DownloadUrlSource`
- `app/src/infrastructure/fs/tokio_file_store.rs`  
  `TokioFileStore` 实现 `FileStore`
- `app/src/infrastructure/event/publisher.rs`  
  `EventBusPublisher` 实现 `TelegramMessagePublisher`
- `app/src/infrastructure/telegram/sender.rs`  
  Telegram 发送器承接实际消息下发

#### 事件系统拆分

当前事件相关代码分成两层：

##### event payload / store adapter

文件：

- `app/src/infrastructure/event/mod.rs`
- `app/src/infrastructure/event/store.rs`
- `app/src/infrastructure/event/publisher.rs`

职责：

- 定义事件 payload（例如 `SendTelegramMessage`）
- 将 payload 写入持久化事件表
- 为应用层提供发布 adapter

##### runtime event bus / worker

文件：

- `app/src/infrastructure/event_bus/mod.rs`
- `app/src/infrastructure/event_bus/worker.rs`

职责：

- 订阅事件名
- 通过 `watch` 通知触发 drain
- 批量读取 pending event
- 重试处理失败事件
- ack 已消费事件

结论：

- 事件机制已经从“直接调用 bot”进化为“持久化事件 + worker delivery”
- 这是当前代码里非常重要的一层解耦
- 但 payload 类型仍放在 infrastructure，不是最终形态

---

### 4.6 `library/import`：当前最大的“半拆分核心”

代码位置：`app/src/library/import.rs` 与 `app/src/library/import/*`

这是目前最需要准确描述、也最容易被误判的一块。

#### 当前它不再是旧式全局状态驱动

`library/import` 现在已经不直接吃 `AppState` 或 `ImportContext` 这类宽对象。  
`Importer<C>` 持有的是：

- `ImportRemote<C>`
- 若干内存缓存（tmdb info / metadata）

而 `ImportRemote<C>` 再依赖 `ImportClient` trait。  
这说明导入链路已经完成了**第一轮端口化**。

#### 但它仍然是混合层

`library/import` 同时包含了：

- 媒体元数据解析后的业务规则
- TMDB 查询策略
- 目录分类与命名规则
- 远端库遍历与目录创建
- 快传 / 下载 / 覆盖 / 删除流程
- 本地 `.strm` 文件写入与删除

也就是说，它不是纯 domain，也不是纯 application，更不是纯 infrastructure。

#### 当前内部的职责分布

- `library/import/metadata.rs`：路径与文件名元数据合并
- `library/import/group.rs`：视频/字幕分组、媒体聚合
- `library/import/category.rs`：影视分类与子类规则
- `library/import/library.rs`：库目录命名和路径计算、列目录
- `library/import/tmdb_info.rs`：TMDB 查询与缓存策略
- `library/import/share.rs`：各分享链接的遍历与导入入口
- `library/import/transfer.rs`：入库、覆盖、快传、删除、strm 落盘
- `library/import/remote.rs`：`ImportClient` trait 与远端能力包装
- `app/src/infrastructure/import/gateway.rs`：把 pan115/pan123/pan189/tmdb concrete client 适配成 `ImportClient`

#### 现状判断

导入链路已经比旧结构健康很多，但仍属于：

> “有端口边界，但业务编排、规则和接入细节还没有完全拆层”

因此，文档里不应再把它描述成“仍然直接依赖所有 concrete client 的泥团”，也不应误写成“已经完全应用层化”。  
**它现在处于中间态。**

---

## 5. 当前依赖关系（按代码实际情况）

当前较准确的依赖方向如下：

```text
main
  -> bootstrap

bootstrap
  -> application services
  -> infrastructure adapters
  -> interface runtime

interface
  -> application services
  -> 少量 runtime context/type alias

application
  -> domain
  -> application ports
  -> 少量 legacy library modules
  -> 少量基础设施事件模型（notify 当前仍有泄漏）

infrastructure
  -> application ports / application traits
  -> domain
  -> legacy library::import model/trait

library/import
  -> domain::media
  -> domain::library::path_mapping
  -> external IO through ImportClient trait
```

所以目前真实情况不是严格的：

```text
interface -> application -> domain
infrastructure -> application::ports
```

而是：

- `sync_strm` 链路已经非常接近这个目标
- `keyword` 和 `download url` 链路基本达到这个目标
- `import` 和 `notify payload` 仍然偏过渡态

---

## 6. 已经完成的重构成果

基于当前代码，以下事情可以视为“已落地”，不应再放在待办区：

### 6.1 目录重组已完成主体迁移

当前已经存在并实际使用：

- `domain/`
- `application/`
- `infrastructure/`
- `interface/`
- `bootstrap/`

这说明重构重点已经不是“改目录名”，而是**继续压缩跨层泄漏**。

### 6.2 `sync_strm` 已从混合逻辑中抽出主干

`app/src/application/sync_strm.rs:1` 已明确体现：

- domain 负责计划计算
- application 负责编排与执行
- infrastructure 提供 remote / fs adapter

这是当前项目最成熟的参考样板。

### 6.3 `keyword` 与 `download url` 已实现 use case + port 模式

对应文件：

- `app/src/application/manage_keywords.rs`
- `app/src/application/resolve_download_url.rs`
- `app/src/application/ports.rs`

这部分已经证明：当前代码库完全能够承受“application service + port + adapter”的模式。

### 6.4 Telegram 发送已通过事件总线异步化

发送链路现在是：

```text
interface/telegram/msg
  -> PublishTelegramMessageService
    -> EventBusPublisher
      -> EventBus / SeaOrmEventStore
        -> EventWorker
          -> DeliverTelegramMessageService
            -> TelegramBotSender
```

这比直接在消息处理里同步发 bot 消息要稳健得多。

### 6.5 `ImportGateway` 已把 concrete client 收口进 infrastructure

`app/src/infrastructure/import/gateway.rs:1` 已经承担了：

- pan123 / pan189 / pan115 / tmdb client 适配
- import 所需模型转换
- `ImportClient` 实现

这意味着 `library/import` 已经不直接依赖这些 concrete client 类型。

---

## 7. 当前仍存在的结构性问题

### 7.1 `AppContext` 仍然是宽 bootstrap 容器

文件：`app/src/bootstrap/app.rs:1`

问题不是它“存在”，而是它当前仍持有过宽的运行时输入：

- DB
- cache
- bot
- event bus
- 所有外部 client
- 所有 import/sync 配置

这在 bootstrap 阶段是可接受的，但会带来两个风险：

- 新功能容易继续往这里堆依赖
- service 组装逻辑容易越来越难测、难替换

建议：

- 保留 `AppContext` 作为 composition root 输入容器
- 但不要让它重新渗透回业务层
- 后续可进一步拆成按 runtime concern 组织的 builder/input structs

### 7.2 `library/import` 仍然混合了规则、编排和接入细节

这是当前最大的剩余重构面。

典型症状：

- `Importer` 同时管媒体识别、TMDB 搜索、路径命名、覆盖判断、远端创建、快传、本地 strm 写入
- `ImportRemote` 除了 remote API 包装，还直接做本地 `.strm` 文件写入和删除
- import-specific 模型与规则没有像 `sync_strm` 一样拆成显式 domain + application 服务

建议：

- 把“分类/命名/覆盖决策/集数缺失计算”继续提纯成 domain 或 import-specific policy 模块
- 把 `Importer` 收缩成 application use case
- 把 `ImportRemote` 拆成更明确的 ports（远端库、分享源、TMDB catalog、本地 strm writer 等）

### 7.3 `application::notify` 仍依赖 infrastructure event payload

文件：`app/src/application/notify.rs:1`

当前 `SendTelegramMessage` 定义在：

- `app/src/infrastructure/event/mod.rs`

这会导致：

- application 向 infrastructure 反向依赖
- payload 难以作为用例层稳定模型演进

建议：

- 将 `SendTelegramMessage` 迁到 `application::notify` 或 `domain::event`
- infrastructure 只负责序列化、持久化和投递

### 7.4 Interface 层仍有少量“组装即使用”的强绑定

例如：

- `interface/http/media.rs` 直接使用 `StringCacheStore` + `Pan123LibraryRemote` type alias
- `interface/telegram/mod.rs` 通过具体 type alias 固定 service concrete type

这不算严重问题，但意味着 interface 还没有做到完全依赖抽象对象。

建议：

- 在当前规模下可以接受
- 若后续入口继续变多，再考虑把 interface runtime context 抽成纯 trait object / generic builder

### 7.5 缺少一套统一的“分层标准”来约束新增代码

当前代码已经呈现出两种风格并存：

- 新风格：`sync_strm` / `resolve_download_url` / `manage_keywords`
- 过渡风格：`library/import` / `notify payload`

如果没有明确约束，新需求很容易继续写回 bootstrap 或 interface。

---

## 8. 建议的目标架构（基于现状渐进演进）

不是推倒重来，而是在现状上继续收口成下面的形态：

```text
interface
  -> application
    -> domain
    -> ports

infrastructure
  -> application::ports
  -> domain

bootstrap
  -> wiring only
```

### 8.1 对 `sync_strm` 链路

保持现状，仅做增量优化：

- 继续补测试
- 避免新逻辑回流到 infrastructure/client 或 interface

### 8.2 对 `keyword` / `download url` 链路

保持现状，可视为模板：

- 新增类似用例时优先复制这种模式

### 8.3 对 `notify` 链路

目标：

- application 拥有消息 payload
- infrastructure 负责 event store / bus / telegram sender

建议最终拆分：

- `application/notify.rs`：message payload + use case
- `infrastructure/event/*`：event store / publisher
- `infrastructure/telegram/*`：delivery adapter

### 8.4 对 `import` 链路

目标不是一次性重写，而是逐段拆。

建议分成 4 类职责：

1. **Import Domain Rules**  
   文件命名、分类、分组、覆盖策略、缺失集计算
2. **Import Application Use Cases**  
   `import_from_share_url` / `import_from_fslink` / `import_from_json`
3. **Import Ports**  
   share source、library remote、metadata catalog、local strm store
4. **Infrastructure Adapters**  
   pan123 / pan189 / pan115 / tmdb / local fs

这条链路可以以 `sync_strm` 为参考模板，而不是另起一套抽象体系。

---

## 9. 分阶段重构建议

### Phase 1：巩固已落地分层

目标：防止回退。

建议动作：

- 新增功能优先走 `application + ports + infrastructure adapter`
- 不再新增直接吃 `AppContext` 的业务逻辑
- 不再把新规则堆进 interface handler

### Phase 2：收口通知链路

目标：消除 application -> infrastructure 反向依赖。

建议动作：

- 迁移 `SendTelegramMessage` 到 application/domain
- 保留 event bus 实现不变
- 用最小 diff 修正 publish / deliver 链路类型归属

### Phase 3：拆分 `library/import`

目标：把当前最大的过渡区拆成清晰层次。

建议优先顺序：

1. 提炼纯规则函数
2. 为 import use case 增加更细的 ports
3. 缩小 `ImportRemote` 的职责
4. 将 `Importer` 改造成 application service，而不是“多职责核心对象”

### Phase 4：瘦身 bootstrap

目标：让 `bootstrap` 只剩 wiring。

建议动作：

- 将 `RuntimeBootstrapInputs` 拆成按 runtime concern 分组的输入结构
- 将 router/bot/service 组装辅助函数继续下沉到专门 builder
- 保持 `AppRuntime::run()` 只负责编排生命周期

---

## 10. 测试策略（按当前代码成熟度）

当前测试结构已经说明项目适合按三层验证：

### 10.1 纯规则测试

适用模块：

- `domain/media/*`
- `domain/library/*`
- `library/import/category.rs`
- `library/import/group.rs`
- `library/import/library.rs` 中的纯命名/路径规则

目标：

- 规则可直接单测
- 不需要 DB、HTTP、Bot

### 10.2 应用层 fake 依赖测试

适用模块：

- `application/sync_strm.rs`
- `application/manage_keywords.rs`
- `application/resolve_download_url.rs`
- `application/import_media.rs`
- `application/notify.rs`

目标：

- 证明 service 编排逻辑不依赖真实外部系统

### 10.3 adapter / runtime 集成测试

适用模块：

- `infrastructure/event_bus/*`
- `interface/http/media.rs`
- `bootstrap` 关键 wiring

目标：

- 覆盖事件 drain、ack、错误映射、路由行为

当前代码已经在这三个方向都有样例，后续只需要沿着已有模式补齐，不需要推翻测试体系。

---

## 11. 一句话结论

当前 BigBrother 的代码现实可以概括为：

> **分层主骨架已经搭起来了，`sync_strm` / `keyword` / `download url` 已基本进入目标态；真正还需要继续拆的是 `library/import` 和少量 notify/bootstrap 边界泄漏。**

所以后续重构重点不应再放在“建新目录”或“发明新抽象”，而应放在：

- 继续把过渡区拆小
- 让 application 拥有更完整的用例边界
- 让 infrastructure 回到纯 adapter 角色
- 让 bootstrap 只负责 wiring
