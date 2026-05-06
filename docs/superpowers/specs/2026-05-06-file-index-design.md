# File Index Design

## 背景

需要把“看到过的资源”记录到数据库，供后续把历史资源转存到 pan123 时搜索使用。资源来源包括 Telegram 消息和手动导入链接。记录发生在解析到文件清单之后，不要求触发 import，也不要求 import 成功。

需要记录和检索的信息包括文件大小、md5、sha1/hash、文件名、文件路径和描述。没有 md5 或 sha1 的文件不入库，因为无法稳定标识文件。

## 目标

- Telegram 授权用户私聊消息和频道监控消息都尝试索引资源，不受 keyword 是否命中影响。
- CLI `import-share-url` 支持 `--description/-d`，并同步写入文件索引。
- 新增 CLI `search-files <keyword>` 搜索历史文件。
- 同一个文件内容可以有多个 `file_name/file_path/description`。
- 同一个文件内容下，相同 `file_name/file_path` 只存一次。
- 相同 description 只存一份，并可关联到多个文件位置。
- 不存 Telegram chat_id、message_id、message_date 等元信息。

## 非目标

- 第一版不做 Telegram 搜索命令。
- 第一版不引入 SQLite FTS，先用 `LIKE` 搜索。
- 第一版不做复杂 hash 合并：如果一次只看到 `size+md5`，另一次只看到 `size+sha1`，系统不能凭空判断它们是同一个文件。只有命中同一 `file_index` 时才补齐缺失 hash。
- 第一版不记录无 hash 文件。

## 推荐方案

使用“文件内容身份 + 文件位置 + 描述去重 + 关联表”的模型。

导入和索引共享来源解析逻辑，但索引写库与媒体识别、TMDB 匹配、pan123 入库解耦。Telegram 通过现有 event 机制异步索引，CLI 同步索引。

## 数据模型

### `file_index`

表示文件内容身份。

字段：

- `id`
- `size`
- `md5` nullable
- `sha1` nullable
- `create_time`
- `update_time`

规则：

- 应用层要求 `size > 0`。
- 应用层要求 `md5` 或 `sha1` 至少一个存在。
- hash 入库前做 `trim` 和小写化，空字符串视为缺失。
- 唯一约束：`(size, md5)`，只对 `md5 IS NOT NULL` 生效。
- 唯一约束：`(size, sha1)`，只对 `sha1 IS NOT NULL` 生效。

查找或插入顺序：

1. 如果有 md5，查 `size + md5`。
2. 如果有 sha1，查 `size + sha1`。
3. 任一命中则复用该 `file_index.id`，并补齐该行缺失的 md5 或 sha1。
4. 都没命中则插入新行。

### `file_location`

表示某个文件内容出现过的文件名和路径。

字段：

- `id`
- `file_index_id`
- `file_name`
- `file_path`
- `location_hash`
- `create_time`
- `update_time`

`location_hash` 计算规则：

```text
sha256("v1\0" + trim(file_path) + "\0" + trim(file_name))
```

规则：

- `file_name/file_path` 原文完整保存，用于展示和搜索。
- `file_path` 允许为空字符串。
- 不对 `file_name/file_path` 做大小写转换，避免误合并大小写有意义的来源。
- 唯一约束：`(file_index_id, location_hash)`。
- 发生唯一冲突时读取现有行并比较原始 `file_name/file_path`；如果不同，视为 hash 碰撞或规范化冲突，返回内部错误，不静默合并。

### `file_description`

表示去重后的描述文本。

字段：

- `id`
- `content_hash`
- `description`
- `create_time`

`content_hash` 计算规则：

```text
sha256(trim(description))
```

规则：

- description 入库前 `trim`。
- 空 description 不入库。
- 原文完整保存。
- `content_hash` 唯一。

### `file_location_description`

表示 description 与具体文件位置的关联。

字段：

- `id`
- `file_location_id`
- `file_description_id`
- `create_time`

规则：

- 唯一约束：`(file_location_id, file_description_id)`。
- 同一个 description 可关联多个 location。
- 同一个 location 可关联多个 description。
- description 绑定到 `file_location`，而不是只绑定到 `file_index`，以保留“哪个文件名/路径对应哪个描述”的上下文。

## 应用层组件

### `FileIndexService`

职责：

- 接收已经解析出的文件清单。
- 过滤没有 hash 或 size 为 0 的文件。
- 批量 upsert `file_index/file_location/file_description/file_location_description`。
- 提供关键词搜索能力。

依赖：

- 新增 `FileIndexRepository` port。

### `FileIndexIngestService`

职责：

- 从分享 URL、fslink、JSON/CAS 文件解析原始文件清单。
- 复用 import 现有 pan123/pan189/pan115 遍历和 JSON/fslink 解析逻辑。
- 将解析结果交给 `FileIndexService`。

边界：

- 它只负责索引，不负责媒体识别、TMDB、转存或 STRM 生成。

### Repository

新增 `infrastructure::repo::file_index`，实现 `FileIndexRepository`，内部使用 SeaORM entity 和 SQLite upsert/查询。

## Telegram 数据流

Telegram 私聊授权用户消息和频道监控消息都会先尝试发布索引事件，然后再走现有 import 判断。

处理流程：

1. Telegram handler 从 text、caption、inline button 中提取分享 URL 和 fslink。
2. 如果消息有 JSON/CAS document，handler 下载到本地 ingest 临时目录。
3. 发布 `IndexFilesFromSource` event。
4. event payload 包含 `sources`、`description`、`source_kind = "telegram"`。
5. event payload 不包含 chat_id、message_id、message_date 等 Telegram 元信息。
6. 后台 worker 订阅事件，调用 `FileIndexIngestService` 解析并写库。
7. 索引事件发布后，Telegram 原有流程继续执行：私聊按授权用户 import，频道按 keyword 决定是否 import。

JSON/CAS document 的本地文件处理：

- handler 下载 document 到 `<data-dir>/ingest/file-index/` 下的临时文件。
- event 中记录本地路径。
- worker 成功处理后删除临时文件。
- worker 失败时保留文件，供事件重试。

错误策略：

- event 发布失败只记录日志，不影响 Telegram 消息处理。
- JSON/CAS 下载失败只记录日志，不影响原有 import 流程。
- worker 遇到临时依赖失败，例如网盘 API 或网络错误，返回错误让现有 event worker 重试。
- worker 遇到永久性输入失败，例如 URL 不支持、JSON 格式错误、无 hash 文件，记录日志并 ack，不反复重试。
- Telegram 不因为 index-only 失败发送失败消息；真正 import 失败仍按现有行为通知。

## CLI 数据流

### `import-share-url`

新增参数：

```text
bigbrother import-share-url --description <text> <url>
```

短参数：

```text
bigbrother import-share-url -d <text> <url>
```

流程：

1. 解析 share URL。
2. 同步调用 `FileIndexIngestService` 写文件索引。
3. 索引失败时打印 warning，继续执行现有 import，除非失败发生在现有 import 也必须失败的基础 URL 解析阶段。
4. 执行现有 import。

CLI 不通过 event 异步索引，命令结束前索引应已经写入数据库。

### `search-files`

新增命令：

```text
bigbrother search-files <keyword>
```

可选参数：

```text
bigbrother search-files --limit 50 <keyword>
```

行为：

- 在 `file_location.file_name`、`file_location.file_path` 和 `file_description.description` 中用 `LIKE` 搜索。
- 默认返回 20 条。
- 输出包含 file name、file path、size、md5、sha1 和匹配到的 description 摘要。
- 空结果输出“未找到匹配文件”。

## 幂等性和去重

同一来源重复处理时：

- 相同 `size + md5` 或 `size + sha1` 复用同一 `file_index`。
- 同一 `file_index` 下相同 `trim(file_path) + trim(file_name)` 复用同一 `file_location`。
- 相同 `trim(description)` 复用同一 `file_description`。
- 相同 location 与 description 的关联只存一次。

同一文件换名或换路径时：

- 复用同一 `file_index`。
- 新增不同的 `file_location`。

同一 description 关联多个文件时：

- 复用同一 `file_description`。
- 为每个 location 新增独立关联。

## 搜索结果形态

第一版搜索结果按 location 展示，而不是只按 file_index 聚合。这样可以看到同一文件在历史消息中出现过的不同文件名和路径。

输出示例：

```text
1. Movie.2026.1080p.mkv
   path: /Movie.2026
   size: 12.34 GB
   md5: ...
   sha1: ...
   description: ...
```

如果一个 location 关联多个 description，CLI 只展示前几条摘要，避免输出过长。

## 测试计划

- `FileIndexService` 过滤无 hash 或 size 为 0 的文件。
- `file_index` 用 `size + md5` 去重。
- `file_index` 用 `size + sha1` 去重。
- 命中已有 `file_index` 时补齐缺失 hash。
- 同一文件不同 `file_name/file_path` 会生成多个 location。
- 同一文件相同 `file_name/file_path` 只生成一个 location。
- 相同 description 只生成一个 `file_description`。
- 同一个 description 可关联多个 location。
- 同一个 location 重复关联同一 description 不重复插入。
- Telegram 私聊授权用户消息发布索引事件。
- Telegram 频道消息即使 keyword 未命中也发布索引事件。
- Telegram index-only 失败不发送失败通知。
- CLI `import-share-url --description` 同步写索引并继续 import。
- CLI `search-files` 可按文件名、路径、description 搜索。
- migration 和 repository 用 SQLite memory DB 测试。
- 完成前运行 `make test` 和 `make lint`。

## 实施顺序建议

1. 新增 migration 和 SeaORM entity。
2. 新增 domain/application 记录模型和 `FileIndexRepository` port。
3. 实现 SeaORM repository 的 upsert 和 search。
4. 抽出共享来源解析能力，供 import 和 index ingest 复用。
5. 实现 `FileIndexService` 和 `FileIndexIngestService`。
6. 接入 CLI 同步索引和 `search-files`。
7. 接入 Telegram 事件发布和 worker。
8. 补齐测试和验证。
