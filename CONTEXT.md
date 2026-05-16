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

## Relationships

- A Telegram message may contain zero or more **Media Sources**
- A **Supported Share Link** is a kind of **Media Source**
- An **Unsupported Link** is not a **Media Source**
- An **Importable Message** contains at least one **Media Source**
- An **Import Failure** can only occur for a **Media Source**, never for an **Unsupported Link**
- A **File Index** contains one or more **File Fingerprints**
- A **File Fingerprint** is identified by file size plus exactly one hash value
- A **File Fingerprint** carries one **File Hash**
- Two different hashes may coexist in the **File Index** even when they refer to the same real-world file

## Example dialogue

> **Dev:** "A forwarded Telegram message includes a TMDB page and one Pan123 share URL. Do both count as media sources?"
> **Domain expert:** "No. The Pan123 URL is a **Supported Share Link**, so the message is **Importable**. The TMDB page is an **Unsupported Link** and should be ignored."

> **Dev:** "Quark reports a file with MD5, and another source later reports what seems to be the same file with SHA1. Should BigBrother merge them?"
> **Domain expert:** "No. Those are two **File Fingerprints** in the **File Index**. BigBrother may show both, because the index records observations, not a canonical logical file."

## Flagged ambiguities

- "无效链接" was ambiguous between **Unsupported Link** and **Import Failure** — resolved: unsupported provider URLs are ignored during source extraction, while failures on supported sources remain visible to the user.
- "文件索引" was ambiguous between a canonical merged file catalog and an observation index — resolved: **File Index** stores separate **File Fingerprints** per observed hash and does not merge MD5 and SHA1 views.
- "`etag`" was ambiguous with HTTP terminology — resolved: the domain concept is **File Hash**, a single algorithm-tagged file fingerprint value.
