# Telegram export file index CLI

We will add a CLI that consumes Telegram Desktop message exports and extracts supported `url` / `fslink` sources for file indexing only. The CLI deduplicates sources within the input file, persists failed source state to a user-specified file, skips succeeded sources by default on rerun, and does not add a database table for this workflow so the recovery boundary stays portable and easy to repair after parser fixes.
