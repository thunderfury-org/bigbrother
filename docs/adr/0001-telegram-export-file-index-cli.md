# Telegram export file index CLI

We will add a CLI that consumes Telegram Desktop message exports and extracts supported `url` / `fslink` sources for file indexing only. The CLI deduplicates sources within the input file, persists processing state in the application database, skips succeeded sources by default on rerun, and may add a dedicated database table for this workflow so recovery lookups and rerun checks stay efficient for large exports.
