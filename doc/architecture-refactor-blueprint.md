# BigBrother 架构重构蓝图（基于当前代码重审）

> 更新时间：2026-04-10  
> 审查范围：`app/src/*`、`migration/src/*`、`Makefile`

## 0. 结论摘要

当前代码已经从“以运行时状态为中心的脚本式实现”演进到了“`bootstrap` 组装 + `application` 用例 + `domain` 规则 + `infrastructure` 适配器 + `interface` 入口”的分层形态，整体方向是对的，而且**主干已经可用**。

但从严格分层和长期可维护性看，项目还处于“**结构已成型、边界未完全收口**”的阶段，主要问题集中在：

1. `bootstrap` 和 `interface` 仍然知道过多具体实现类型。
2. import 链路已经拆层，但内部仍偏大、偏深，理解成本高。
3. 错误模型过粗，很多外部失败被压平为 `AppError::Internal`。
4. 测试总量不错，但偏单元测试，缺少更高一层的用例/装配回归测试。

当前建议不是再做一次“大重写”，而是沿现有结构继续收口，优先做**边界去具体化、import 链路瘦身、测试分层补齐**。

---

## 1. 架构

### 1.1 当前实际分层

当前 `app` crate 的真实结构可以概括为：

```text
main
  -> bootstrap
    -> interface
      -> application
        -> domain
      -> infrastructure
  -> background runtime

migration
```

对应目录：

- 入口：`app/src/main.rs`
- 运行时装配：`app/src/bootstrap/app.rs`、`app/src/bootstrap/mod.rs`
- 用例层：`app/src/application/*`
- 领域层：`app/src/domain/*`
- 基础设施层：`app/src/infrastructure/*`
- 入口适配层：`app/src/interface/*`
- 数据迁移：`migration/src/*`

### 1.2 启动与运行时链路

启动过程较清晰：

1. `main` 只解析 CLI，并进入 `server` 子命令。
2. `AppContext::new` 负责创建底层依赖：
   - 配置
   - SQLite 连接
   - Telegram Bot
   - Cache
   - EventBus
   - pan115/pan123/pan189/tmdb client
3. `AppRuntime::from_app` 负责把依赖装配成可运行服务：
   - Telegram bot runtime
   - HTTP media server
   - event delivery worker
   - cache cleanup loop
4. `AppRuntime::run` 负责执行 migration 并并发拉起运行时任务。

这说明项目已经不再是“业务逻辑直接共享一个巨大的全局状态”，而是**先构造依赖，再注入到具体用例中运行**。

### 1.3 已经比较成熟的部分

#### 领域层

`domain` 中有两块已经比较像稳定内核：

- `domain/media`
  - 文件名解析、规范化、标题/语言/季集/画质提取
  - 规则密集，基础设施依赖少
  - 测试覆盖较多
- `domain/library`
  - 路径映射
  - `.strm` 同步计划构建
  - 输入输出明确，纯函数比例高

这两块说明：**项目最有价值的规则已经开始沉淀到 domain，而不是散落在入口或 client 中。**

#### 应用层

以下 service 边界已经相对清楚：

- `ManageKeywordsService`
  - 标准 CRUD 用例服务
- `ResolveDownloadUrlService`
  - 先查缓存，再查远端，再做结果映射
- `SyncStrmService`
  - 采集远端/本地快照，调用 domain 生成同步计划，再执行写入与清理
- `PublishTelegramMessageService` / `DeliverTelegramMessageService`
  - 把“发消息”拆成发布与投递两个用例
- `ImportMediaService`
  - 已经成为 import 的应用入口，而不是从 interface 直接穿透到一堆 client

### 1.4 当前架构的主要问题

#### 问题一：`bootstrap` 仍是“总装配中心 + 知识汇聚点”

`bootstrap/mod.rs` 当前承担了过多职责：

- 创建具体 adapter
- 决定 service 组合方式
- 决定 interface 层所见的具体类型
- 决定后台任务拓扑

这在工程初期合理，但随着模块增多，`bootstrap` 会持续膨胀，并成为“修改一个用例时必须同步理解的地方”。

#### 问题二：`interface` 仍然直接绑定基础设施具体类型

例如 Telegram runtime 中直接写死了：

- `SeaOrmKeywordRepository`
- `PanLibraryGateway`
- `ShareImportGateway`
- `TmdbMetadataGateway`
- `FilesystemImportLocalStore`
- `Pan123LibraryRemote`
- `TokioFileStore`

这说明 interface 虽然不直接操作数据库/HTTP，但它仍然**知道太多实现细节**。现在这些类型别名只是在 interface 内部隐藏了一层，没有真正实现“按端口依赖”。

#### 问题三：import 链路虽然分层，但内部复杂度仍高

import 相关代码已经从旧路径迁到：

- `application/import_media.rs`
- `application/import/*`
- `domain/import/*`
- `infrastructure/import/*`

这是明显进步；但 import 仍然是当前系统最复杂的一块，表现为：

- 文件体量大
- 模块跳转深
- workflow 与策略逻辑交织
- 同一用例跨越 metadata、share、transfer、cleanup、save 多个模块

这类设计短期可运行，长期会拖慢理解和改动效率。

#### 问题四：领域边界尚不完全统一

目前存在两种并行风格：

- 一部分功能已经是“domain 纯规则 + application 编排”
- 另一部分仍是“application 子模块自己承接不少规则细节”

典型表现是 import 相关规则分散在：

- `domain/import/policy.rs`
- `application/import/group.rs`
- `application/import/transfer_support.rs`
- `application/import/transfer.rs`

边界不算错，但还没有收敛成一致心智模型。

### 1.5 推荐的下一阶段架构方向

建议继续沿现在的结构演进，而不是推翻重来。

#### 阶段 A：先把接口层和装配层收口

目标：

- `interface` 只依赖 runtime 暴露的 service，不依赖基础设施具体类型
- `bootstrap` 只负责装配，不承载业务判断

建议动作：

1. 为 `BotRuntime`、`MediaServerContext` 引入更抽象的 service 字段类型。
2. 把 `interface/telegram/mod.rs` 中的具体 type alias 逐步下沉到 `bootstrap`。
3. 让 `bootstrap` 输出更稳定的 runtime DTO / context，而不是一堆具体泛型实现。

#### 阶段 B：继续压缩 import 用例复杂度

目标：

- `ImportMediaService` 仍做总入口
- `application/import/*` 只保留编排
- 规则进一步下沉到 `domain/import`

建议动作：

1. 以“媒体识别 / 分组 / 传输决策 / 本地落盘”重新梳理 import 子模块边界。
2. 将 `group.rs`、`transfer_support.rs` 中稳定规则逐步下沉到 domain。
3. 给 import 引入更明确的中间模型，减少函数间隐式约定。

#### 阶段 C：统一后台任务模型

当前后台任务包括：

- HTTP server
- Telegram dispatcher
- event delivery
- cache cleanup

建议后续为它们引入统一的 runtime task 抽象，至少做到：

- 生命周期一致
- 日志入口一致
- shutdown 行为一致
- 失败策略一致

这能避免未来任务数增加后 `AppRuntime::run` 继续膨胀。

---

## 2. 代码质量

### 2.1 总体评价

代码质量整体处于“**中上，可持续演进，但仍需结构性收口**”的状态。

优点：

- 模块命名基本统一
- 用例层大量采用 trait 端口
- 纯规则模块开始沉淀到 domain
- 已经有不少 fake-backed 单元测试
- Workspace、migration、运行命令都比较清楚

主要风险：

- 大文件较多
- 错误抽象偏粗
- 若干边界上仍有具体实现泄漏
- import 子系统理解成本偏高

### 2.2 目前最明显的质量优势

#### 优势一：已经有明确的“端口 + 适配器”趋势

`application/ports.rs`、`application/import_ports.rs` 把用例对外部系统的依赖抽成了接口，这使得：

- `SyncStrmService` 可以对远端文件系统和本地文件系统做 fake 测试
- `ResolveDownloadUrlService` 可以独立于 cache/source 测试
- `ManageKeywordsService` 可以独立于数据库测试
- import service 可以在不依赖真实网盘的情况下回归

这是当前代码最值得保留和强化的设计资产。

#### 优势二：领域规则并未继续往 interface/infrastructure 扩散

像文件名解析、同步计划、导入策略等规则，已经主要停留在 `domain` 或 `application/import`，没有继续扩散到 Telegram/HTTP handler 中。说明架构方向是健康的。

#### 优势三：重构不是“纯目录迁移”，而是已经有行为收敛

比如 `SyncStrmService` 并不只是换个目录，而是把：

- 快照采集
- 同步计划生成
- 执行写入/删除

分成了较清晰的阶段。这类演进比“只是把文件挪到新目录”更有价值。

### 2.3 当前主要质量问题

#### 问题一：大文件过多，热点集中

当前体量最大的模块包括：

- `app/src/domain/import/policy.rs`
- `app/src/infrastructure/client/pan123.rs`
- `app/src/application/import_media.rs`
- `app/src/infrastructure/client/http.rs`
- `app/src/domain/media/parser.rs`
- `app/src/application/sync_strm.rs`

这些大文件不一定说明实现错误，但会带来：

- 理解成本高
- 局部修改时回归面大
- 容易让测试与生产代码混在一起放大文件体积

其中最需要优先瘦身的是 **import 链路** 和 **pan123/http client**。

#### 问题二：错误模型过于扁平

`AppError` 目前只有：

- `InvalidParameter`
- `NotFound`
- `Internal`

这对项目初期够用，但随着模块增多，问题会逐渐显现：

- application 无法保留足够上下文
- interface 只能做有限映射
- 外部依赖失败、领域规则失败、装配失败都容易被压平

建议逐步引入分层错误语义，例如：

- domain 级校验错误
- external dependency 错误
- use case 执行错误
- startup/runtime 错误

不是为了做复杂错误体系，而是为了避免“所有东西都变成 internal error”。

#### 问题三：运行时代码里存在较多 `unwrap` / `expect`

仓库中有不少 `unwrap`，其中一部分只是测试代码或 `LazyLock<Regex>` 初始化，这没问题；但运行路径里仍有一些值得收敛的点，例如：

- 启动时对 migration、初始化、listener 绑定直接 `expect/unwrap`
- Telegram user id 转换直接 `unwrap`
- 若干文件路径父目录推断直接 `unwrap`

这些点不一定马上出故障，但会让运行时错误以 panic 形式暴露，不利于服务稳定性与问题定位。

建议原则：

- 测试代码可接受 `unwrap`
- 静态正则初始化可接受 `unwrap`
- 运行时 IO/配置/网络相关路径尽量返回 `AppResult`

#### 问题四：interface 层承担了部分流程判断

以 Telegram 消息处理为例，`MsgProcessor` 不只是“翻译输入”，还承担了：

- URL/秒传/JSON 的识别与分流
- 发送过程提示消息
- 部分去重与流程控制

这部分并非错误，但继续增长后会让 interface 变成“半个应用层”。建议后续把“消息解析后的命令语义”整理成更明确的 application 输入模型。

### 2.4 代码质量改进优先级

建议按下面顺序推进：

1. **收敛装配边界**  
   把 interface 对具体 adapter 的认知继续下沉。

2. **拆小 import 热点文件**  
   优先减轻 `import_media.rs`、`group.rs`、`transfer_support.rs` 的认知负担。

3. **细化错误语义**  
   至少区分启动错误、外部依赖错误、用例错误。

4. **减少运行路径 panic 点**  
   将关键 `unwrap/expect` 改为显式错误返回和日志。

5. **建立模块规模红线**  
   新增逻辑避免继续堆入超大文件。

---

## 3. 测试

### 3.1 当前测试现状

当前测试基础明显好于很多同规模项目。

本次审查已执行：

```bash
cargo test -q
```

结果：

- `154 passed`
- `0 failed`

测试分布也比较健康，覆盖了：

- `domain/media` 文件名解析与归一化
- `domain/library` 路径与同步计划
- `domain/import` share/fslink/策略逻辑
- `application` service 的 fake-backed 测试
- `interface/http/media` 的 handler 行为
- `infrastructure/cache`、`event_bus`、`client/http` 等适配层

### 3.2 当前测试的优点

#### 优点一：核心规则层已有较强单元测试

尤其是：

- 媒体解析
- 导入策略
- 路径/同步计划

这些最容易因“看似小改动”被破坏的地方，已经有比较扎实的保护网。

#### 优点二：应用层已经可用 fake 做隔离测试

这是当前测试设计最重要的价值。说明端口抽象不是摆设，而是真正帮助用例测试脱离外部依赖。

#### 优点三：基础设施层也有必要的回归测试

例如：

- cache store
- event bus / worker
- HTTP client

这避免了“只有纯函数有测试，真正会坏的适配层没人测”的常见问题。

### 3.3 当前测试的缺口

#### 缺口一：缺少更高层的装配回归测试

目前大多数测试停在模块级、service 级。缺少几类更高层验证：

- `bootstrap` 装配是否完整
- runtime 关键依赖能否顺利连通
- 主要 handler 与 service 的装配兼容性

不一定要做完整端到端，但至少应有少量“thin integration test”覆盖主链路。

#### 缺口二：import 主流程的场景测试还可以继续补

虽然 import 已有不少回归测试，但它仍是最复杂、最容易退化的部分。建议补强：

- 混合字幕/视频/重复文件场景
- 多来源 share 的边界行为
- 已存在目标文件时的覆盖/跳过策略
- metadata 缺失或歧义时的处理

#### 缺口三：运行时异常路径测试较少

当前大多数测试偏成功路径或单点失败路径，建议后续增加：

- 配置错误
- migration 失败
- event handler 连续失败
- 文件系统权限问题
- 外部 client 返回非法数据

这些更接近真实线上故障。

### 3.4 推荐的测试策略

后续建议采用三层测试结构：

#### 第一层：domain 规则测试

继续保持高密度、小粒度、快速执行。

适合对象：

- parser
- policy
- path mapping
- sync plan

#### 第二层：application 用例测试

继续使用 fake / stub adapter，验证：

- 编排顺序
- 错误传播
- 状态变化
- 输出摘要

适合对象：

- `SyncStrmService`
- `ImportMediaService`
- `ResolveDownloadUrlService`

#### 第三层：少量集成测试

只保留少量但关键的“穿层”测试，重点保护主链路：

- HTTP redirect 主路径
- Telegram message -> import service 的一条关键流程
- event publish -> delivery 的链路
- migration + repository 的基本可用性

目标不是追求数量，而是补上当前“模块都测了，但装起来是否工作”这一层空白。

---

## 4. 分阶段重构建议

### P0：立即可做

1. 收敛 `interface` 对具体基础设施类型的直接认知。  
2. 为 import 子模块建立更稳定的中间模型和命名边界。  
3. 处理运行路径上的关键 `unwrap/expect`。  

### P1：下一轮重构

1. 拆小 import 热点文件。  
2. 细化错误模型。  
3. 增加少量运行时装配/集成测试。  

### P2：中期优化

1. 统一后台任务抽象。  
2. 继续把稳定规则从 application 下沉到 domain。  
3. 根据变化频率拆分 pan123/http client 内部模块。  

---

## 5. 最终判断

这份代码的当前状态，不应被定义为“架构混乱”，更准确的判断是：

**主干架构已经建立，且方向正确；现在最大的任务不是重做，而是把已经出现的好边界继续收紧。**

如果继续沿当前路线演进，优先解决：

- 装配层过宽
- interface 对具体实现泄漏
- import 子系统复杂度
- 测试层级不均衡

那么这个项目可以比较平滑地进入“**业务继续增长，但结构仍可控**”的阶段。

---

## 6. 可执行任务清单

下面的任务按“可单独提交 PR”来组织，目标是让后续重构更容易排期、拆分和验收。

### 6.1 PR-1：收敛 interface 对具体实现的依赖

目标：

- 让 `interface` 只面向 service 能力，不直接感知具体 adapter 类型

建议改动：

1. 重构 `app/src/interface/telegram/mod.rs`
   - 去掉面向具体实现的 type alias
   - 让 `BotRuntime` 持有更稳定的 service 字段
2. 重构 `app/src/interface/http/media.rs`
   - 让 `MediaServerContext` 只依赖下载地址解析 service
   - 避免在 interface 层重新拼具体 cache/source 组合
3. 把具体装配逻辑集中到 `app/src/bootstrap/mod.rs`

验收标准：

- `interface/*` 中不再出现 `SeaOrmKeywordRepository`、`PanLibraryGateway`、`TokioFileStore` 这类基础设施实现名
- `cargo test` 通过

### 6.2 PR-2：拆小 import 入口和 workflow

目标：

- 降低 import 主链路的阅读和修改成本

建议改动：

1. 拆分 `app/src/application/import_media.rs`
   - 保留 service 门面
   - 将测试辅助和场景回归拆到更独立位置
2. 继续整理 `app/src/application/import/mod.rs`
   - 明确 factory、use case、workflow 三层职责
3. 给 import 中间模型补统一命名
   - 避免同类概念在不同模块中重复表达

验收标准：

- `import_media.rs` 明显瘦身
- import 主链路文件职责更单一
- 现有 import 回归测试继续通过

### 6.3 PR-3：把稳定规则继续下沉到 domain/import

目标：

- 让 application/import 更聚焦编排

建议改动：

1. 评估并下沉 `app/src/application/import/group.rs` 中稳定规则
2. 评估并下沉 `app/src/application/import/transfer_support.rs` 中稳定规则
3. 为迁移后的纯规则补直接单元测试

优先迁移的规则类型：

- 文件分组
- 文件名决策
- 覆盖策略
- 集数缺失计算

验收标准：

- `application/import/*` 中的纯规则函数数量下降
- `domain/import/*` 的测试覆盖提升

### 6.4 PR-4：细化错误模型

目标：

- 避免所有异常都被压成 `AppError::Internal`

建议改动：

1. 扩展 `app/src/error.rs`
2. 为 application 层补更有语义的错误映射
3. 为 interface 层补更稳定的错误到响应/消息映射

建议优先区分：

- 参数错误
- 外部依赖错误
- 业务规则拒绝
- 启动/运行时错误

验收标准：

- 关键用例不再依赖字符串拼接区分错误类型
- HTTP / Telegram 入口的错误表现更一致

### 6.5 PR-5：减少运行路径 panic 点

目标：

- 降低服务因配置、IO、转换异常直接 panic 的概率

建议改动：

1. 清理 `main`、`bootstrap`、`interface/http` 中的关键 `unwrap/expect`
2. 对路径父目录、监听地址、用户 ID 转换等点改为显式错误返回
3. 保留测试代码中的 `unwrap`

验收标准：

- 运行路径上的关键 `unwrap/expect` 数量明显下降
- 失败时日志可定位具体原因

### 6.6 PR-6：补运行时装配级测试

目标：

- 填补“模块都测了，但装起来是否工作”这一层空白

建议改动：

1. 为 event publish -> delivery 增加一条装配链路测试
2. 为 HTTP redirect 主路径增加一条更接近运行时的测试
3. 为 import 主链路增加一条更高层的 thin integration test

验收标准：

- 新增测试不依赖真实外部服务
- 覆盖至少 2 条跨层主链路

---

## 7. issue 拆分建议

如果要进一步拆成 issue，建议至少建下面 8 个：

1. `refactor: decouple telegram runtime from concrete adapters`
2. `refactor: decouple media http context from concrete adapter wiring`
3. `refactor: slim import media service and workflow factory`
4. `refactor: move stable import rules into domain`
5. `refactor: introduce richer app error taxonomy`
6. `refactor: remove panic-prone unwrap/expect in runtime path`
7. `test: add thin integration tests for runtime chains`
8. `docs: keep architecture blueprint in sync with refactor progress`

---

## 8. 建议执行顺序

推荐顺序如下：

1. 先做 `PR-1`
   - 因为收益高、风险相对可控
2. 再做 `PR-5`
   - 能先提升运行稳定性
3. 然后做 `PR-2` + `PR-3`
   - 集中处理 import 复杂度
4. 再做 `PR-4`
   - 在边界相对稳定后细化错误模型
5. 最后做 `PR-6`
   - 用装配级测试给前面的重构兜底

这样推进，可以避免一开始就在 import 和错误体系上同时大动干戈，降低单轮重构风险。
