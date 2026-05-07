# Pan123 新分享链接格式 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- []`) syntax for tracking.

**Goal:** 支持 Pan123 新分享链接格式 `https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw#`

**Architecture:** 修改 `ShareUrl::from()` 的 host 和 path 匹配逻辑，新增测试覆盖新格式。

**Tech Stack:** Rust, url crate

---

### Task 1: Host 和 Path 匹配 + 测试

**Files:**
- Modify: `app/src/domain/import/source.rs:15-22` (`ShareUrl::from` 方法)

- [ ] **Step 1: 新增测试用例**

在 `app/src/domain/import/source.rs` 的 `tests` 模块中，更新 `share_url_recognizes_supported_hosts` 测试，加入新格式 URL：

```rust
#[test]
fn share_url_recognizes_supported_hosts() {
    let pan123 = Url::parse("https://www.123pan.com/s/abc123?pwd=test").unwrap();
    let pan123_new = Url::parse("https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw").unwrap();
    let pan189 = Url::parse("https://cloud.189.cn/t/abc123").unwrap();

    assert!(matches!(ShareUrl::from(&pan123), Some(ShareUrl::Pan123(_))));
    assert!(matches!(ShareUrl::from(&pan123_new), Some(ShareUrl::Pan123(_))));
    assert!(matches!(ShareUrl::from(&pan189), Some(ShareUrl::Pan189(_))));
}
```

在 `parses_share_specific_parts` 测试中，加入新格式的解析验证：

```rust
let pan123_new = Url::parse("https://1850081502.share.123865.com/123pan/4Ulmvd-hWbSA?pwd=33Rw").unwrap();
assert_eq!(
    parse_pan123_share_parts(&pan123_new),
    ("4Ulmvd-hWbSA".into(), "33Rw".into())
);
```

- [ ] **Step 2: 运行测试确认失败**

运行: `cargo test --lib domain::import::source::tests::share_url_recognizes_supported_hosts`
预期: FAIL — 新格式 URL 返回 None

- [ ] **Step 3: 修改 host 和 path 匹配逻辑**

将 `ShareUrl::from` 中 Pan123 的条件从：

```rust
if url
    .host_str()
    .is_some_and(|host| host.starts_with("www.123") && host.ends_with(".com"))
    && url.path().starts_with("/s/")
```

改为：

```rust
if url
    .host_str()
    .is_some_and(|host| {
        (host.starts_with("www.123") || host.contains(".share.123"))
            && host.ends_with(".com")
    })
    && (url.path().starts_with("/s/") || url.path().starts_with("/123"))
```

- [ ] **Step 4: 运行测试确认通过**

运行: `cargo test --lib domain::import::source::tests`
预期: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/domain/import/source.rs
git commit -m "support new Pan123 share link format with share.123 domain"
```
