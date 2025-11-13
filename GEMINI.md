# BigBrother: Gemini Code Companion

This document provides a comprehensive overview of the BigBrother project, designed to assist developers and future AI interactions in understanding and working with this codebase.

## Project Overview

BigBrother is a backend application written in Rust. Its primary purpose is to manage and organize TV show files and metadata. The application integrates with The Movie Database (TMDB) for metadata and supports cloud storage management.

The project consists of two main components that run concurrently:

*   **Web Server:** An `axum`-based web server that exposes a media server.
*   **Telegram Bot:** A `teloxide`-based bot that listens for messages and channel posts, likely for notification or remote control purposes.

The application is configured using a `config.yaml` file and uses a SQLite database for persistence, managed by the `sea-orm` crate.

## Building and Running

### Building

To build the project, use the standard Cargo command:

```bash
cargo build
```

For a release build, use:

```bash
cargo build --release
```

### Running

The application is launched via the command line. You need to specify the path to the data directory, which contains the configuration file (`config.yaml`) and the database.

```bash
# Run in debug mode
cargo run -- server --data-dir /path/to/your/data/directory

# Run a release binary
./target/release/bigbrother server --data-dir /path/to/your/data/directory
```

### Testing

The project does not appear to have a dedicated test suite in the files that were analyzed. If tests are added, they can be run using:

```bash
cargo test
```

## Development Conventions

*   **Asynchronous:** The entire codebase is asynchronous, using the `tokio` runtime.
*   **Configuration:** Application configuration is managed through a `config.yaml` file and loaded at startup. The `config.rs` module defines the configuration structure.
*   **Error Handling:** The project uses the `thiserror` crate for custom error types, which can be found in `src/error.rs`.
*   **Logging:** The `tracing` crate is used for logging. Logs are initialized and managed in the `logger.rs` module.
*   **Modularity:** The code is organized into modules by feature (e.g., `bot`, `server`, `client`, `library`).
*   **HTTP Client:** `reqwest` is used for making HTTP requests to external services.
*   **Database:** `sea-orm` is used as the ORM for database interactions.
*   **Telegram Bot:** The `teloxide` crate is used for the Telegram bot functionality.
*   **Web Framework:** The `axum` framework is used for the web server.
