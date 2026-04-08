# BigBrother 架构重构蓝图

## 1. 背景

当前仓库的目录划分本身不混乱，`bot`、`server`、`client`、`library`、`media` 这些模块名也基本符合职责范围。但核心问题不在“目录名字”，而在“依赖边界”和“运行时对象穿透方式”。

当前实现里，`AppState` 同时持有数据库、缓存、Telegram Bot、外部 HTTP client、配置和事件总线，并被业务逻辑大范围透传。结果是：

- 业务逻辑依赖的是一个全家桶对象，而不是最小能力集
- 单个用例常常同时耦合数据库、网络、文件系统和消息发送
- 很多逻辑只能写成集成测试，单元测试和用例测试难度高
- 一旦要替换实现或注入 fake/mock，改动面很大

这份文档的目标是给出一套可落地的整体重构方案，把当前项目演进成“边界清晰、测试友好、渐进迁移”的结构。

## 1.1 当前落实情况（2026-04-08）

根据当前代码状态，这份蓝图里的前半段工作已经有了实质落地：

- `application/ports.rs` 已经存在，`sync_strm`、`manage_keywords`、`resolve_download_url` 等用例已经通过应用层 service 暴露边界。
- `domain/library/path_mapping.rs` 与 `domain/library/sync_plan.rs` 已承担同步链路里的纯规则计算，`SyncStrmService` 主要负责编排与执行。
- `main.rs` 已直接通过 `bootstrap::AppRuntime`、`interface::{http, telegram}`、`infrastructure::event_bus::EventBus` 组装和运行系统，不再依赖顶层 `bot` / `server` / `event_bus` 入口。
- `domain` / `application` / `infrastructure` / `interface` 四层目录都已落地，`media` 已迁入 `domain/media`，`client`、`entity`、`cache` 也已收口到 `infrastructure/*`。
- `interface/http/media.rs`、多个 `application/*` 模块以及 `domain/media` / `infrastructure/event_bus` 已有直接测试覆盖，说明“先抽边界再测用例”的方向已经不仅可行，而且正在成为默认结构。

但这轮 review 也确认了几处还未完全收口的耦合点：

- `bootstrap/app.rs` 里的 `AppContext` 仍然一次性持有 DB、cache、bot、event bus、全部 client 和配置，本质上还是 bootstrap 阶段的宽依赖容器。
- `import` 链路已经移除了 `ImportContext` 这类打包依赖对象；当前 `ImportGateway` 已通过 import 专用 model + trait adapter 把 Pan/TMDB concrete client 隔离在 infrastructure 内，`library/import` 不再直接依赖这些类型。
- `library/import` 的主干已经从“宽依赖容器驱动”推进到“显式 port 驱动”，当前剩余重点已转向继续压缩 bootstrap 宽上下文与补齐运行时/worker 侧测试证据。

## 1.2 本轮 review 的代码定位

为了避免后续继续围绕抽象描述打转，这里把本轮 review 对应到当前代码位置：

- [`app/src/main.rs`](../app/src/main.rs)：当前唯一的 crate 根入口，已经直接声明 `bootstrap`、`interface`、`infrastructure` 等蓝图目标模块。
- [`app/src/bootstrap/mod.rs`](../app/src/bootstrap/mod.rs) 与 [`app/src/bootstrap/app.rs`](../app/src/bootstrap/app.rs)：当前真正的 composition root / bootstrap container；其中 `AppContext` 已收缩为 `RuntimeBootstrapInputs` 的提供者，`AppRuntime::run()` 已统一承接 server / telegram / event bus / cache cleanup 生命周期编排。
- [`app/src/interface/telegram/mod.rs`](../app/src/interface/telegram/mod.rs)、[`app/src/interface/http/mod.rs`](../app/src/interface/http/mod.rs)、[`app/src/interface/cli/mod.rs`](../app/src/interface/cli/mod.rs)：入口层目录已经收口到蓝图目标结构。
- [`app/src/infrastructure/client/`](../app/src/infrastructure/client/)、[`app/src/infrastructure/cache/`](../app/src/infrastructure/cache/)、[`app/src/infrastructure/entity/`](../app/src/infrastructure/entity/)：接入型代码已不再留在顶层 root。
- [`app/src/library/import/`](../app/src/library/import)：端口化已经继续向前推进；该目录已改为依赖 import 专用 model 与 `ImportClient` trait，而不是直接依赖具体 Pan/TMDB client。

## 2. 当前问题总结

### 2.1 `AppState` / `AppContext` 仍像 service locator

当前 [`app/src/bootstrap/app.rs`](../app/src/bootstrap/app.rs) 里的 `AppContext` 负责：

- 读取配置
- 初始化数据库连接
- 初始化缓存
- 初始化多个外部 client
- 初始化事件总线
- 初始化 Telegram Bot

随后运行时组装仍需要从这个宽上下文中分发依赖。

这会带来两个直接后果：

- 业务函数的真实依赖不显式，阅读签名看不出它到底需要 DB、网络、缓存还是 Bot
- 测试必须先造出一个“足够完整”的 `AppContext`，哪怕只想验证一小段业务规则

### 2.2 业务流程和 IO 操作混在一起

例如 `library::sync` 这条链路里，同时做了：

- 远端目录遍历
- 文件类型识别
- 本地路径映射
- 生成 `.strm`
- 下载字幕
- 删除本地脏文件

这些动作跨越了远端盘 API、本地文件系统和业务规则，集中在同一个对象中。结果是测试一个局部行为时，不得不搭整条链路。

### 2.3 interface 层过胖

Telegram 命令处理和 HTTP handler 里不仅在做输入输出映射，也直接做了业务编排和数据访问。这样会导致：

- handler 难以单测
- 业务规则重复散落在入口层
- 同一个用例很难复用到 CLI、Bot、HTTP 多入口

### 2.4 基础设施对象没有被抽象成端口

例如：

- Pan123
- TMDB
- SeaORM repository
- tokio 文件系统
- teloxide Bot
- 事件存储与消费

这些都直接被业务代码依赖。缺少稳定的 trait 边界，就很难替换实现，也很难给应用层写 fake。

### 2.5 测试分层不清晰

仓库里已经有不少纯逻辑测试，这是好现象；但对于跨模块用例，缺少明确的分层策略：

- 哪些属于纯单元测试
- 哪些属于用例测试
- 哪些属于仓储/客户端集成测试
- 哪些属于 handler 测试

结果就是“能测的地方测得还行，难测的地方基本不测”。

## 3. 重构目标

重构后的目标不是追求术语完整，也不是把所有东西都 trait 化，而是实现以下四点：

1. 业务逻辑不再依赖 `AppState`
2. 外部系统通过 trait 端口注入
3. interface 层只做输入输出适配
4. 测试按层次组织，绝大部分业务可在不连真实外设的情况下验证

## 4. 目标架构

建议将应用整理为四层。

### 4.1 Domain 层

职责：

- 纯业务模型
- 纯规则
- 不依赖数据库、网络、文件系统、Bot、HTTP 框架

适合放入 Domain 的内容：

- 媒体元数据解析与归一化
- 视频/字幕分组规则
- 媒体导入分组规则
- 远端路径到本地路径的映射规则
- 同步计划计算
- 下载 URL 解析后的业务约束

要求：

- 不依赖 `tokio`
- 不依赖 `SeaORM`
- 不依赖 `reqwest`
- 不依赖 `teloxide`
- 尽量做到同步、纯函数、易测试

### 4.2 Application 层

职责：

- 编排用例
- 调用 domain 规则
- 通过 trait 访问外部能力
- 负责事务边界、用例级错误汇总、流程控制

典型 use case：

- `SyncStrmService`
- `ImportMediaService`
- `ManageKeywordService`
- `ResolveDownloadUrlService`
- `PublishTelegramNotificationService`

Application 层只依赖：

- domain
- 一组端口 trait

### 4.3 Infrastructure 层

职责：

- 外部系统接入
- trait 的具体实现
- 框架适配

包括：

- `SeaOrmKeywordRepository`
- `SeaOrmCacheStore`
- `SeaOrmEventStore`
- `Pan123HttpClient`
- `TmdbHttpClient`
- `TokioFileStore`
- `TelegramBotNotifier`
- `SqlEventBusWorker`

Infrastructure 可以依赖：

- `SeaORM`
- `reqwest`
- `tokio::fs`
- `teloxide`
- `axum`

但这些都不应直接渗透到 application 里。

### 4.4 Interface 层

职责：

- 接收外部输入
- 做参数解析和校验
- 调用 application service
- 把 application 结果映射成 HTTP/Telegram/CLI 响应

包括：

- Telegram command handler
- Telegram callback handler
- Axum route handler
- CLI 命令入口

规则：

- 不直接操作数据库
- 不直接拼业务流程
- 不直接访问外部 client

## 5. 依赖方向

重构后依赖必须单向流动：

```text
interface -> application -> domain
interface -> application -> ports
infrastructure -> application::ports
infrastructure -> domain
```

禁止：

- domain 依赖 infrastructure
- application 依赖具体 ORM/client/Bot 类型
- interface 直接依赖 repository/client 并编排业务

## 6. 目录建议

建议收敛为以下目录结构：

```text
app/src/
  main.rs
  bootstrap/
    mod.rs
    app.rs
  domain/
    mod.rs
    media/
      mod.rs
      parser.rs
      normalize.rs
      grouping.rs
    library/
      mod.rs
      sync_plan.rs
      path_mapping.rs
      import_grouping.rs
    keyword/
      mod.rs
    event/
      mod.rs
  application/
    mod.rs
    ports.rs
    sync_strm.rs
    import_media.rs
    manage_keywords.rs
    resolve_download_url.rs
    notify.rs
  infrastructure/
    mod.rs
    client/
      mod.rs
      pan123.rs
      tmdb.rs
      http.rs
    repo/
      mod.rs
      keyword.rs
      cache.rs
      event.rs
    fs/
      mod.rs
      tokio_fs.rs
    telegram/
      mod.rs
      notifier.rs
    event_bus/
      mod.rs
      worker.rs
  interface/
    mod.rs
    telegram/
      mod.rs
      command_handler.rs
      callback_handler.rs
    http/
      mod.rs
      media.rs
    cli/
      mod.rs
```

说明：

- 现有 `media` 下的纯逻辑优先迁进 `domain/media`
- 现有 `library` 下的纯规则迁进 `domain/library`
- 现有 `client`、`entity`、`cache` 等接入型代码已迁进 `infrastructure`
- 原 `bot` 和 `server` 入口型代码已迁进 `interface`
- `bootstrap/app.rs` 已替代旧式全局状态入口；后续目标是继续缩小它，而不是恢复新的全家桶上下文
- 当前真正剩余的结构性问题已经不再是“目录名”；`library/import` 的端口化已经落地，后续重点是继续收缩 `bootstrap/app.rs` 的宽上下文，并为事件/worker 运行时补齐测试证据

## 7. 端口设计原则

trait 不要按“技术名词”来切，而要按“用例所需能力”来切。避免设计出一个什么都能做的巨型接口。

### 7.1 推荐端口

#### LibraryRemote

用于同步与导入过程中访问远端盘：

- `find_root(path) -> Result<Option<RemoteId>>`
- `list_dir(dir_id) -> Result<Vec<RemoteEntry>>`
- `download_file(file_id, local_path) -> Result<()>`
- `get_download_url(file_id) -> Result<String>`
- `fast_upload(...) -> Result<Option<FileId>>`

#### MetadataCatalog

用于 TMDB 等元数据查询：

- `find_movie(metadata) -> Result<Option<MovieDetail>>`
- `find_tv(metadata) -> Result<Option<TvDetail>>`

#### KeywordRepository

- `list_all() -> Result<Vec<Keyword>>`
- `add(value) -> Result<()>`
- `delete(id) -> Result<()>`

#### CacheStore

- `get<T>(key) -> Result<Option<T>>`
- `set<T>(key, value, ttl) -> Result<()>`
- `clear_expired() -> Result<u64>`

#### FileStore

用于本地文件系统抽象：

- `read_to_string(path) -> Result<String>`
- `write(path, bytes) -> Result<()>`
- `metadata(path) -> Result<FileMetadata>`
- `create_dir_all(path) -> Result<()>`
- `read_dir(path) -> Result<Vec<DirEntry>>`
- `remove_file(path) -> Result<()>`
- `remove_dir_all(path) -> Result<()>`

#### Notifier

用于业务通知输出：

- `send_message(user, text, reply_to) -> Result<()>`

#### EventStore

- `append(name, payload) -> Result<()>`
- `list_unacked(name, limit) -> Result<Vec<EventRecord>>`
- `ack(id) -> Result<()>`

### 7.2 不推荐的接口

不建议设计：

- `trait AppServices`
- `trait AppStateLike`
- `trait Repository` 包含所有实体操作
- `trait Pan123AndTmdbAndFs`

这些接口会把现在的问题换个名字继续保留。

## 8. `AppState` 的新定位

`AppState` 不应该继续作为“业务依赖对象”。建议把它重命名或收缩成 `BootstrapApp` / `ApplicationContext`，只负责：

- 读取配置
- 创建具体依赖实现
- 组装 service
- 把 service 注入到 interface 层

重构后，业务函数签名应该像这样：

```rust
pub struct SyncStrmService<R, F, C> {
    remote: R,
    file_store: F,
    cache: C,
    config: SyncConfig,
}

impl<R, F, C> SyncStrmService<R, F, C> {
    pub async fn execute(&self) -> AppResult<SyncSummary> {
        // ...
    }
}
```

而不是：

```rust
pub async fn sync_strm(state: &AppState) -> AppResult<()>
```

## 9. 按现有模块的迁移建议

### 9.1 `media`

现状：

- 已经有不少纯逻辑
- 测试基础相对最好

处理建议：

- 保持为第一批稳定资产
- 迁入 `domain/media`
- 保证不引入 DB、HTTP、异步依赖

### 9.2 `library::sync`

现状：

- 业务规则、远端访问、本地文件操作揉在一起

目标拆法：

1. `domain/library/path_mapping.rs`
   - 远端路径转本地路径
   - `.strm` 路径生成

2. `domain/library/sync_plan.rs`
   - 输入远端目录快照 + 本地目录快照
   - 输出同步计划：
     - 创建哪些 `.strm`
     - 更新哪些字幕
     - 删除哪些脏文件
     - 删除哪些脏目录

3. `application/sync_strm.rs`
   - 负责调用 `LibraryRemote`、`FileStore`、`CacheStore`
   - 将远端数据转为 plan
   - 执行 plan

这样测试可以分三层：

- path mapping 的纯函数测试
- sync plan 的纯数据测试
- service 的 fake 依赖测试

### 9.3 `library::import`

现状：

- 业务价值最高，也是最复杂的一块
- 当前内部已经有一部分纯逻辑和一部分外部依赖混在一起

建议拆分：

1. `domain/library/import_grouping.rs`
   - 文件分组
   - 剧集归并
   - 命名/路径决策

2. `application/import_media.rs`
   - 编排 Pan123、TMDB、Repository、FileStore

3. `infrastructure/client/*`
   - 保持具体 HTTP 调用和 token 管理

其中需要重点避免的一点是：不要让 `Importer` 继续直接持有整个 `AppState`。

### 9.4 `bot`

现状：

- Telegram handler 同时做鉴权、业务编排、数据库读写和消息回发

目标拆法：

1. `interface/telegram/*`
   - 解析命令
   - 读取用户输入
   - 调用 application service
   - 渲染输出文案

2. `application/manage_keywords.rs`
   - 提供列出、添加、删除关键字等用例

3. `application/sync_strm.rs`
   - 由 Telegram 入口复用，不在 handler 里直接落业务

### 9.5 `server/media`

现状：

- handler 里直接访问 cache 和 Pan123 client

目标拆法：

1. `application/resolve_download_url.rs`
   - 负责缓存命中
   - 缓存未命中时获取下载地址
   - 转换为统一结果类型

2. `interface/http/media.rs`
   - 只负责把结果映射成 HTTP 状态码和重定向

### 9.6 `event_bus`

现状：

- 事件持久化、通知、订阅、后台消费、重试都在一起

目标拆法：

1. `EventStore`
   - 纯存储

2. `EventPublisher`
   - 负责写入事件

3. `EventProcessor`
   - 负责读取并执行 handler

4. `Worker`
   - 负责后台循环和 tokio spawn

5. `RetryPolicy`
   - 控制重试间隔

这样业务逻辑可以同步测试，后台 worker 单独测。

## 10. 测试体系重建方案

重构的核心收益必须落实到测试上。建议明确四类测试。

### 10.1 Domain 单元测试

目标：

- 最高比例
- 最快速度
- 覆盖纯规则

包括：

- 文件名解析
- 分组规则
- 路径映射
- 同步计划生成
- 导入决策逻辑

特点：

- 不连数据库
- 不跑网络
- 不用 tokio runtime 也尽量能测

### 10.2 Application 用例测试

目标：

- 使用 fake/mock 端口
- 验证业务流程

包括：

- 同步远端目录后是否生成正确计划
- 缓存 miss 时是否会回源并写缓存
- 删除关键字失败时是否返回正确错误
- 导入流程在 TMDB miss 时是否跳过或降级

特点：

- 不连真实外部系统
- 主要用内存 fake

### 10.3 Infrastructure 集成测试

目标：

- 验证适配器是否正确

包括：

- HTTP client + `wiremock`
- repository + sqlite memory
- file store + tempdir

特点：

- 数量少于 domain/application 测试
- 但每类适配器要有代表性覆盖

### 10.4 Interface 测试

目标：

- 验证参数解析和响应映射

包括：

- HTTP handler 状态码
- Telegram command 到 use case 的映射
- callback data 解析

特点：

- 不做复杂业务断言
- 不重复 application 已验证过的逻辑

## 11. 分阶段迁移计划

不建议一次性推倒重写。建议分六个阶段。

### 阶段 1：引入 application 服务层

目标：

- 保持行为不变
- 先把业务入口从 `AppState` 直连改成 service 调用

动作：

- 新建 `application/`
- 为 `sync_strm`、`keyword`、`resolve_download_url` 建 service
- interface 层先改为依赖 service

验收：

- 行为不变
- 编译通过
- 现有测试通过

### 阶段 2：抽出端口 trait

目标：

- 从 application 对具体实现解耦

动作：

- 建立 `ports.rs`
- 先从最关键的 `LibraryRemote`、`CacheStore`、`KeywordRepository`、`FileStore` 开始
- infrastructure 实现这些端口

验收：

- application 不再 import 具体 client/repo 类型

### 阶段 3：拆 `sync` 链路

目标：

- 让同步逻辑可测试

动作：

- 抽 path mapping
- 抽 sync plan 纯逻辑
- service 只负责编排与执行

验收：

- `sync` 有清晰的 domain/application 测试

### 阶段 4：拆 `keyword` 与 `resolve download url`

目标：

- 让 bot/http 入口瘦下来

动作：

- bot 只负责 command parsing 和 response rendering
- HTTP handler 只负责状态码映射

验收：

- handler 可以通过 fake service 测试

### 阶段 5：拆 `import`

目标：

- 处理最复杂用例

动作：

- 分离纯规则与外部调用
- 把 `Importer` 改成显式依赖端口
- 去掉对 `AppState` 的直接持有

验收：

- `import` 的核心流程可以用 fake remote/tmdb/repo 测

### 阶段 6：重构事件系统与运行时组装

目标：

- 让后台系统可测试、可替换

动作：

- 引入 `EventStore`、`EventProcessor`、`Worker`
- `main.rs` 只做 bootstrap

验收：

- 运行时初始化逻辑清晰
- 事件处理可做同步测试和 worker 测试

## 12. 每阶段建议补的测试

### 阶段 1

- service 的冒烟测试

### 阶段 2

- fake port 驱动的 application 测试

### 阶段 3

- `sync_plan` 纯数据测试
- 路径映射测试
- `SyncStrmService` 用例测试

### 阶段 4

- `ManageKeywordService` 测试
- `ResolveDownloadUrlService` 测试
- HTTP handler 状态码测试

### 阶段 5

- 导入分组纯逻辑测试
- 导入 service fake 测试

### 阶段 6

- `EventProcessor` 重试与 ack 测试
- worker 生命周期测试

## 13. 风险与控制措施

### 13.1 最大风险

- 边改边动功能，导致行为回归
- trait 切分过粗或过细
- `import` 链路拆分不当，短期复杂度反而上升
- 事件系统改造期间出现消费语义变化

### 13.2 控制措施

- 每阶段只解决一个方向的问题
- 先收口依赖，再优化实现
- 纯规则优先下沉到 domain
- 先补回归测试，再移动复杂逻辑
- 每个阶段结束都执行 `fmt`、`lint`、`test`

## 14. 明确不做的事

本次重构不建议同时做以下事情：

- 不引入新依赖只为 mock
- 不先拆成多 crate
- 不把所有模块都泛型化
- 不追求完整 DDD 战术建模
- 不在第一阶段就重写所有命名和目录

目标是“建立清晰边界并逐步迁移”，不是制造新的抽象噪音。

## 15. 推荐实施顺序

按收益和风险比排序，建议实际执行顺序如下：

1. 引入 application services
2. 给关键用例抽 ports
3. 重构 `sync`
4. 重构 `resolve download url`
5. 重构 `keyword` 管理
6. 重构 `import`
7. 重构 `event_bus`
8. 最后统一整理目录和命名

## 16. 完成后的预期效果

如果按这份蓝图完成重构，预期收益应当是：

- 绝大部分业务逻辑可以在不连外部系统的情况下测试
- handler/入口层明显瘦身
- `AppState` 不再是全局耦合中心
- 新增功能时可以优先写 application/domain 测试
- 替换 Pan123、TMDB、缓存、通知实现的成本明显降低
- 后续如果需要拆 crate 或做更大规模演进，也有更稳定的边界可依赖

## 17. 下一步落地建议

如果按照当前代码状态继续稳妥推进，下一轮工作已经可以非常聚焦，不需要再重复目录整理：

1. 让 `bootstrap/app.rs` 里的 `AppContext` 继续收缩成纯 bootstrap 容器，避免继续承担业务依赖访问入口的角色。
2. 继续清理 `library/import` 与相关入口中对具体 infrastructure 类型的直连命名，让应用层和接口层更稳定地只依赖抽象。
3. 为 `event` worker 生命周期、重试语义和运行时装配补齐更直接的测试证据。
4. 当 `import` 链路的端口化稳定后，再考虑是否要进一步细化 `bootstrap/app.rs` 的职责边界。

这样更符合仓库现在的真实阶段：目录与命名基本已经落到蓝图目标，`import` 这条核心链路也已经完成从“宽依赖容器驱动”到“显式端口驱动”的推进；接下来更高价值的工作是继续收缩 bootstrap 宽上下文，并补强运行时/worker 的测试证据。
