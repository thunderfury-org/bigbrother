# BigBrother

BigBrother ingests media-related sources from Telegram messages, imports supported share contents into a local library, and keeps playback-facing artifacts in sync. This context distinguishes between message noise, supported import sources, and actual import failures so operators receive actionable feedback without Telegram spam.

## Language

**Media Source**:
An input extracted from a Telegram message that BigBrother can attempt to process.
_Avoid_: link, attachment, payload

**Supported Share Link**:
A share URL from a provider BigBrother explicitly knows how to resolve into raw files.
_Avoid_: valid URL, effective link

**Unsupported Link**:
A URL found in a Telegram message that is not a share URL from any provider BigBrother supports.
_Avoid_: invalid share link, failed import

**Importable Message**:
A Telegram message that contains at least one **Media Source** after source extraction and filtering.
_Avoid_: valid message, usable post

**Source Record**:
One upstream input carrier from which BigBrother can extract zero or more **Media Sources**.
A **Source Record** may be a live Telegram message, a channel post, or a historical export record.
_Avoid_: raw event, payload, telegram-only message

**Media Source Observation**:
One concrete **Media Source** extracted from one **Source Record** and treated as an independently trackable processing unit.
_Avoid_: task URL, whole-message job

**Source Message Link**:
A Telegram jump link that points back to the original channel message from which a **Media Source** was extracted.
It is one unified field at the application boundary, but the concrete URL format depends on whether the channel has a public username.
_Avoid_: fixed channel URL, reply link

**Import Failure**:
A failure that occurs while processing a **Media Source** BigBrother recognizes and has decided to handle.
_Avoid_: ignored link, unsupported link

**File Fingerprint**:
A file identity observation recorded from a single hash algorithm together with file size.
One **File Fingerprint** carries either MD5 or SHA1, but not both.
_Avoid_: logical file, merged file identity

**File Hash**:
The single observed hash attached to a file observation, represented as one algorithm-tagged value rather than parallel optional fields.
_Avoid_: etag, dual-hash identity

**File Index**:
The searchable collection of observed **File Fingerprints** and their seen locations.
It does not merge different hashes into one record, even if they may refer to the same real file.
_Avoid_: deduplicated file catalog, canonical file identity

**Source Tag**:
A short release-scene token that describes distribution channel or source and must not become part of a media title candidate.
_Avoid_: title word, release group

**Subscription**:
A user-registered TMDB target (a movie or a whole TV series) that acts as the sole positive-allow gate for **Media Source** import. A **Subscription** is purely passive: BigBrother does not actively reach out to external systems to find content for it. It is applied at three points:
  - new channel-post **Source Record** ingest: two layers.
      * Layer 1 (cheap prefilter): the message description must contain at least one subscription's title text (`title_zh` or `title_en`).
      * Layer 2 (precise): after raw file parsing and TMDB lookup, the resolved `tmdb_id` must match a subscription.
  - manual rescan: best-effort scan of the **File Index** by subscription title text, then verified via TMDB lookup; misses are expected (e.g. obfuscated file names won't be found).
  - direct messages bypass both layers and are imported regardless of subscriptions (treated as explicit operator intent).
_Avoid_: follow, watch, auto-tracking (these imply active fetching, which this system does not do); keyword (which this concept replaces).

## Relationships

- A Telegram message may contain zero or more **Media Sources**
- A **Source Record** may yield zero or more **Media Sources**
- A **Supported Share Link** is a kind of **Media Source**
- An **Unsupported Link** is not a **Media Source**
- An **Importable Message** contains at least one **Media Source**
- One **Media Source Observation** is extracted from exactly one **Source Record**
- One **Media Source Observation** contains exactly one **Media Source**
- One **Media Source Observation** may carry zero or one **Source Message Link**
- An **Import Failure** can only occur for a **Media Source**, never for an **Unsupported Link**
- A **File Index** contains one or more **File Fingerprints**
- A **File Fingerprint** is identified by file size plus exactly one hash value
- A **File Fingerprint** carries one **File Hash**
- Two different hashes may coexist in the **File Index** even when they refer to the same real-world file
- A **Source Tag** may appear inside a raw media filename but is not part of any parsed title candidate
- A channel-post **Media Source** is imported only if both its description text matches some **Subscription** title AND its resolved `tmdb_id` is covered by a **Subscription**
- A direct-message **Media Source** is imported regardless of **Subscriptions**

## Example dialogue

> **Dev:** "A forwarded Telegram message includes a TMDB page and one Pan123 share URL. Do both count as media sources?"
> **Domain expert:** "No. The Pan123 URL is a **Supported Share Link**, so the message is **Importable**. The TMDB page is an **Unsupported Link** and should be ignored."

> **Dev:** "Quark reports a file with MD5, and another source later reports what seems to be the same file with SHA1. Should BigBrother merge them?"
> **Domain expert:** "No. Those are two **File Fingerprints** in the **File Index**. BigBrother may show both, because the index records observations, not a canonical logical file."

## Flagged ambiguities

- "无效链接" was ambiguous between **Unsupported Link** and **Import Failure** — resolved: unsupported provider URLs are ignored during source extraction, while failures on supported sources remain visible to the user.
- "文件索引" was ambiguous between a canonical merged file catalog and an observation index — resolved: **File Index** stores separate **File Fingerprints** per observed hash and does not merge MD5 and SHA1 views.
- "`etag`" was ambiguous with HTTP terminology — resolved: the domain concept is **File Hash**, a single algorithm-tagged file fingerprint value.
- "导入 JSON" was ambiguous between ingesting a prebuilt index dump and replaying historical Telegram exports — resolved: this flow consumes a **Source Record** and tracks work per **Media Source Observation**.
- "`DSNP` 这类词" was ambiguous between title text and source metadata — resolved: treat it as a **Source Tag**, not as a title candidate or release group.
