# Pan123 新分享链接格式支持

## Context

Pan123 新增了一种分享链接格式：

```
https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw#
```

现有代码只能识别旧格式 `https://www.123pan.com/s/<key>?pwd=<pwd>`，新格式的 host 和 path 模式不同，无法通过 `ShareUrl::from()` 检测。

## 范围

- **只需修改 URL 检测逻辑**，提取出 ShareKey + SharePwd 后，现有 Pan123 API (`/api/share/get`) 可正常工作。
- `parse_pan123_share_parts()` 不需要修改（`next_back()` 取最后一段路径，两种格式都能正确提取 share key）。

## 改动

### 1. `ShareUrl::from()` — Host 匹配 (`app/src/domain/import/source.rs:14-22`)

当前逻辑：
```rust
host.starts_with("www.123") && host.ends_with(".com")
```

新增条件：
```rust
host.contains(".share.123") && host.ends_with(".com")
```

`.share.123` 组合足够独特，无需额外提取 label。

### 2. `ShareUrl::from()` — Path 匹配 (`app/src/domain/import/source.rs:19`)

当前逻辑：
```rust
path.starts_with("/s/") || path.starts_with("/d/")
```

新增 `/123` 前缀（覆盖 `/123pan/`）：
```rust
path.starts_with("/s/") || path.starts_with("/d/") || path.starts_with("/123")
```

### 3. 新增测试

- 新 URL 格式的 `ShareUrl::from()` 识别
- 新 URL 格式的 `parse_pan123_share_parts()` 解析（share key + password 提取）

## 不改动

- `parse_pan123_share_parts()` — 已兼容两种格式
- Pan123 API 客户端 — ShareKey/SharePwd 参数不变
- Import pipeline — 不受影响
- Telegram URL 提取 — 正则已能匹配新 URL
