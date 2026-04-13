# BigBrother 架构重构蓝图（2026-04-11 复审版）

> 更新时间：2026-04-11  
> 审查范围：`app/src/*`、`migration/src/*`、`Makefile`  
> 验证结果：`cargo test -q` -> `161 passed`

## 0. 结论摘要

当前代码已经从“分层已成型”推进到了“**主干重构基本完成**”的阶段。

本次复审后的总体判断：

1. 代码结构已经稳定在 `bootstrap + interface + application + domain + infrastructure` 的分层模型上。
2. `interface` 对具体基础设施实现的直接耦合已明显下降，主要残余压力集中在 `bootstrap/mod.rs`。
3. import 子系统已经完成多轮拆分，不再是单文件热点问题，而是“模块多、跳转深、心智复杂”的问题。
4. 错误模型已经完成第一轮细化，`AppError` 不再是扁平结构。
5. 测试已经补到更高一层，装配链路不再完全空白。

因此，当前已经不适合再把项目描述为“架构尚未落地”；更准确的表述是：

**主干架构已经落地，第一轮蓝图任务已大体完成；后续工作应从“大重构”切换到“围绕热点和尾项持续优化”。**

---

## 1. 当前架构复盘

### 1.1 当前采用的架构模型

当前项目最贴近的描述是：

- **分层架构**
- **用例驱动（application-centric）**
- **端口 / 适配器（ports & adapters）**
- 带有明显的 **DDD-lite / Clean Architecture / Hexagonal Architecture** 风格

大致依赖关系为：

```text
main
  -> bootstrap
    -> interface
      -> application
        -> domain
      -> infrastructure
```

对应目录：

- 入口：`app/src/main.rs`
- 装配：`app/src/bootstrap/*`
- 用例层：`app/src/application/*`
- 领域层：`app/src/domain/*`
- 基础设施层：`app/src/infrastructure/*`
- 输入输出适配层：`app/src/interface/*`
- 迁移：`migration/src/*`

### 1.2 运行时链路

当前启动链路清晰：

1. `main` 只负责 CLI 分发。
2. `AppContext::new` 负责创建运行时输入：
   - 配置
   - DB
   - Bot
   - Cache
   - EventBus
   - 外部 client
3. `AppRuntime::from_app` 负责把运行时输入装配成：
   - Telegram runtime
   - HTTP media server
   - event delivery runtime
   - cache cleanup runtime
4. `AppRuntime::run` 负责：
   - migration
   - 并发启动 runtime
   - shutdown 收尾

这说明项目已经不是“全局状态驱动的脚本式服务”，而是一个清晰的“先构造依赖，再装配运行时”的系统。

### 1.3 当前最成熟的结构

#### 领域层

当前最像稳定内核的是：

- `domain/media`
  - 文件名解析
  - 元数据归一化
  - 标题/语言/季集/画质提取
- `domain/library`
  - 路径映射
  - `.strm` 同步计划
- `domain/import/policy.rs`
  - 导入分组
  - 覆盖策略
  - 集数缺失计算
  - 文件命名决策

这些模块的共同点：

- 基础设施依赖少
- 规则密度高
- 单元测试较完整
- 可以作为长期稳定演进的核心

#### 应用层

应用层目前职责相对明确：

- `ManageKeywordsService`
- `ResolveDownloadUrlService`
- `SyncStrmService`
- `PublishTelegramMessageService`
- `DeliverTelegramMessageService`
- `ImportMediaService`

特别是 `ImportMediaService` 已经退化成门面 service，复杂度主要下沉到 `application/import/*` 与 `domain/import/*`，这是明显进步。

---

## 2. 当前主要问题

### 2.1 `bootstrap` 仍然过宽

当前最大的结构性热点已经不是 `interface`，而是 `bootstrap/mod.rs`。

它仍然同时负责：

- 具体适配器选择
- service 组合
- runtime 拓扑
- 并发任务管理
- migration 前后流程

这意味着：

- 新增运行时能力时仍然容易集中修改 `bootstrap/mod.rs`
- 装配知识尚未被进一步压缩
- 后续如果 runtime 增长，`AppRuntime::from_app` 和 `AppRuntime::run` 仍有继续膨胀的风险

### 2.2 import 子系统已拆小，但认知复杂度仍高

当前 import 已完成多轮拆分，主要体现在：

- `ImportedMedia` 独立建模
- `transfer` 拆成：
  - `movie`
  - `tv`
  - `season`
  - `episode`
- `transfer_save` 拆成：
  - `upload`
  - `finalize`
- `tmdb_info` 拆成：
  - `id`
  - `resolve`
  - `tests`
- 多个大测试块迁出主文件

因此 import 的问题已经从：

- “文件过大”

转为：

- “模块很多”
- “跳转深”
- “编排逻辑和规则逻辑尚未完全形成单一心智模型”

这说明 import 已经更可维护，但仍是整个系统最复杂的业务子域。

### 2.3 领域边界正在收口，但还没有完全统一

本轮复审确认：已有稳定规则继续从 `application/import` 下沉到 `domain/import/policy.rs`，例如：

- 覆盖/跳过策略
- 分组规则
- 文件命名策略
- 缺失集数计算

但仍有一部分规则留在：

- `application/import/group.rs`
- `application/import/transfer_support.rs`

所以当前状态不是“边界混乱”，而是：

**边界已经明显收口，但还没有完全统一。**

### 2.4 interface 层仍然承担了一些流程判断

以 `interface/telegram/msg.rs` 为例，当前它仍负责：

- URL / fslink / JSON 的识别与分流
- 输入文本提取
- 结果消息拼装
- 处理前后提示消息

这不算架构错误，但如果继续增长，`MsgProcessor` 可能演化成“半个应用层”。

后续建议：

- 继续把“输入语义识别后的命令模型”向 application 收拢
- 让 interface 更偏输入翻译，而不是承担持续增长的流程判断

---

## 3. 代码质量复盘

### 3.1 总体评价

当前代码质量可评价为：

**中上偏强，主干稳定，可继续中期演进。**

优点：

- 模块命名统一
- 用例层 ports 边界清晰
- 领域规则沉淀在持续发生
- 可测试性较好
- import 重构不是“纯挪文件”，而是伴随真实职责收敛

当前主要风险：

- 少数热点文件仍偏大
- `bootstrap/mod.rs` 偏重
- import 仍然是全系统理解成本最高的部分

### 3.2 目前最值得肯定的点

#### 用例层 ports 是真实生效的

`application/ports.rs`、`application/import_ports.rs` 不是形式化接口，而是真正支撑了：

- fake-backed service tests
- cache/source 隔离测试
- import 门面回归测试
- event publishing / delivery 测试

这说明“端口”在当前项目里是实用设计资产，而不是文档摆设。

#### 错误模型已不再扁平

`AppError` 目前已支持：

- `InvalidParameter`
- `NotFound`
- `Dependency`
- `RuleRejected`
- `Runtime`
- `Internal`

并且：

- HTTP 入口已基于错误语义映射状态码
- Telegram 入口已基于错误语义映射提示文案

这比上一轮“很多错误都被压平为 `Internal`”已经前进了一大步。

#### 运行路径上的高风险 panic 已下降

这轮之后，关键运行路径中一些明显的 `unwrap/expect` 已经被替换成显式错误返回，例如：

- migration
- runtime task join
- tmdb id 解析
- 关键启动路径错误传播

虽然尚未清零，但运行稳定性相比上一版明显改善。

### 3.3 当前剩余质量热点

当前仍然值得优先关注的热点文件包括：

- `app/src/domain/import/policy.rs`
- `app/src/domain/media/parser.rs`
- `app/src/infrastructure/client/pan123.rs`
- `app/src/infrastructure/client/http.rs`
- `app/src/application/sync_strm.rs`
- `app/src/bootstrap/mod.rs`

这些热点不一定都要立刻拆，但它们代表了未来最容易再次积累复杂度的位置。

---

## 4. 测试复盘

### 4.1 当前测试基线

本次复审实际执行：

```bash
cargo test -q
```

结果：

- `161 passed`
- `0 failed`

### 4.2 当前测试覆盖的强项

目前测试覆盖比较稳的区域包括：

- `domain/media`
- `domain/library`
- `domain/import`
- `application` service fake-backed tests
- `interface/http/media`
- `infrastructure/cache`
- `infrastructure/event_bus`
- `infrastructure/client/http`

也就是说，当前项目已经不是“只有纯函数有测试”的状态。

### 4.3 当前测试缺口

#### 装配级测试已经起步，但还不够厚

这轮已经补上了：

- `event publish -> delivery` 的装配链路测试

因此“模块都测了，但装起来没人测”这件事已经不再完全成立。

但仍然缺少：

- 更接近运行态的 HTTP 主路径测试
- import 门面到 workflow 的更抽象薄集成测试
- bootstrap 层自身的轻装配校验

#### 异常路径测试仍偏少

当前更值得继续补的异常路径包括：

- 配置错误
- migration 失败
- 外部 client 返回异常数据
- 文件系统边界情况
- 长链路中的 partial failure

---

## 5. 对蓝图原任务的复核结论

### PR-1：收敛 interface 对具体实现的依赖

结论：**已完成**

当前 `interface/*` 中已经基本不再直接出现基础设施实现名，具体装配主要已压到 `bootstrap/services.rs` 与 `bootstrap/mod.rs`。

### PR-2：拆小 import 入口和 workflow

结论：**已完成**

当前 import 主链路已经完成较大幅度拆分，`import_media.rs` 保持门面化，主入口和 workflow 的职责比上一版清晰很多。

### PR-3：把稳定规则继续下沉到 domain/import

结论：**已部分完成**

已有一批稳定规则下沉进 `domain/import/policy.rs`，但 `group.rs` 与 `transfer_support.rs` 里仍有可进一步迁移的规则。

### PR-4：细化错误模型

结论：**已完成第一轮**

错误模型已经从扁平结构升级为分层语义结构，但仍需要继续统一错误构造风格，减少历史性 `Internal`。

### PR-5：减少运行路径 panic 点

结论：**已完成第一轮**

关键启动和运行路径已处理一批高风险 panic 点，但仍有少量剩余点位值得继续清理。

### PR-6：补运行时装配级测试

结论：**已完成第一轮**

已经补上 event chain 的装配级测试，但还没有形成更完整的 runtime thin integration 覆盖面。

---

## 6. 下一阶段建议

### 6.1 短期优先项

1. 继续完成 `PR-3` 尾项
   - 继续把 `application/import` 中剩余稳定规则迁入 `domain/import`
2. 继续完成 `PR-4` / `PR-5` 尾项
   - 统一错误构造方式
   - 清理剩余关键运行路径 panic 点
3. 补 `PR-6` 第二轮
   - 为 HTTP 主路径和 import 门面增加更贴近运行态的薄集成测试

### 6.2 中期优化项

1. 拆 `pan123/http client` 内部热点
2. 继续压缩 `bootstrap/mod.rs`
3. 评估统一 runtime task 抽象
4. 为大模块建立规模红线，避免复杂度反弹

---

## 7. 最终判断

当前 BigBrother 不应再被描述为“架构尚未落地”或“重构刚开始”。

更准确的判断是：

**主干架构已经稳定，第一轮重构已基本完成；接下来最重要的不是推翻重来，而是持续清理热点、补齐尾项、稳步降低复杂度。**

如果继续按当前路线推进，这个项目有较大机会进入：

**业务继续增长，但结构仍然可控** 的状态。
