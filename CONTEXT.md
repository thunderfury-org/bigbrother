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

## Relationships

- A Telegram message may contain zero or more **Media Sources**
- A **Supported Share Link** is a kind of **Media Source**
- An **Unsupported Link** is not a **Media Source**
- An **Importable Message** contains at least one **Media Source**
- An **Import Failure** can only occur for a **Media Source**, never for an **Unsupported Link**

## Example dialogue

> **Dev:** "A forwarded Telegram message includes a TMDB page and one Pan123 share URL. Do both count as media sources?"
> **Domain expert:** "No. The Pan123 URL is a **Supported Share Link**, so the message is **Importable**. The TMDB page is an **Unsupported Link** and should be ignored."

## Flagged ambiguities

- "无效链接" was ambiguous between **Unsupported Link** and **Import Failure** — resolved: unsupported provider URLs are ignored during source extraction, while failures on supported sources remain visible to the user.
