# ADR-0002: Subscription as Sole Import Gate

## Status

Accepted

## Context

The project imports media from cloud drives (115, 123, 189 pan) via Telegram channel posts and direct messages. Previously, a keyword system provided ad-hoc substring matching to decide which files to import. Keywords were unstructured strings with no link to TMDB metadata, causing false positives and maintenance overhead.

Following the identify/import split in #120, the import pipeline now has a clear boundary between identification (TMDB lookup) and transfer. Issue #121 replaces the keyword system with a `Subscription` model — a user-registered TMDB target (movie or TV series) that acts as the sole positive-allow filter.

## Decision

### Subscription as whitelist

A `Subscription` row records a `tmdb_id`, `title_zh`, and `title_en`. Nothing is imported unless it matches a subscription. This is a breaking change: the `keyword` table is dropped by migration, and all legacy keyword data is lost.

### Two-layer channel-post filtering

- **Layer 1 (cheap prefilter):** The message description must contain at least one subscription's `title_zh` or `title_en` as a case-sensitive substring. Messages that fail Layer 1 are skipped entirely — no file parsing or TMDB lookup occurs. This replaces the old keyword system.
- **Layer 2 (precise):** After raw file parse and TMDB lookup, the resolved `tmdb_id` must exist in the subscription table. Files that fail Layer 2 are silently skipped — no `ImportRecord` is written.

### Direct-message bypass

DMs bypass both layers. They are treated as explicit operator intent and flow straight to identify + import.

### File-level granularity

Filtering operates at the file level, not the message level. A single post may contain files that hit different subscriptions or none. Only matched files proceed; unmatched files are silently skipped. Files where TMDB lookup yields no match still flow through the existing Unmatched channel.

### Empty subscription table

When the subscription table is empty, all channel posts are filtered out at Layer 1. This is intentional: the user must explicitly register subscriptions before any channel-post import occurs.

### Manual rescan

Rescan searches the File Index by subscription title text (both `title_zh` and `title_en`), identifies candidates via TMDB lookup, then keeps only groups whose resolved `tmdb_id` matches the Subscription being rescanned. Known limitation: files with obfuscated names that do not match any subscription title are missed.

A rescan is one processing unit. All File Index hits are identified and imported together so TV episodes group under one title, and the run writes one ImportRecord. Per-fingerprint import (the #121 loop) was rejected because it is slow, floods import history, and makes a single episode look like `S01 1/N` with the rest of the season marked missing. Concurrency without grouping was rejected for the same history problem.

### Keyword system removal

The entire keyword stack was deleted: `ManageKeywordsService`, `KeywordRepository`, SeaORM entity/repo, and the `/keyword`, `/addkeyword`, `/deletekeyword` bot commands. No migration path is provided.

## Consequences

- Import is opt-in. A fresh installation with an empty subscription table imports nothing from channels until the user adds subscriptions.
- False positives from loose keyword matching are eliminated; every import is backed by a verified TMDB ID.
- The loss of keyword data is a known breaking change communicated in release notes.
- Rescan has a blind spot: files with names unrelated to any subscription title are not discovered, even if they match a subscribed TMDB ID. This can be addressed later by building a TMDB ID index over the File Index.
