use bootstrap::{AppContext, AppRuntime};
use clap::Parser;
use error::AppResult;
use interface::{
    cli::{Cli, Commands},
    import::{NO_NEW_MEDIA_MESSAGE, format_import_summaries, format_verbose_import_notes},
};
use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use url::Url;

mod application;
mod bootstrap;
mod config;
mod domain;
mod error;
mod infrastructure;
mod interface;
mod logger;
mod util;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Server(args) => {
            if let Err(err) = run_server(args.data_dir.as_str()).await {
                eprintln!("Failed to start server: {err}");
                std::process::exit(1);
            }
        }
        Commands::ImportShareUrl(args) => {
            if let Err(err) = run_import_share_url(
                args.data_dir.data_dir.as_str(),
                &args.url,
                args.verbose,
                args.description.clone(),
            )
            .await
            {
                eprintln!("Failed to import from share url: {err}");
                std::process::exit(1);
            }
        }
        Commands::SearchFiles(args) => {
            if let Err(err) =
                run_search_files(args.data_dir.data_dir.as_str(), &args.keyword, args.limit).await
            {
                eprintln!("Failed to search files: {err}");
                std::process::exit(1);
            }
        }
    }
}

async fn run_server(data_dir: &str) -> AppResult<()> {
    let app = AppContext::new(data_dir).await?;
    AppRuntime::from_app(app)?.run().await
}

async fn run_import_share_url(
    data_dir: &str,
    url: &str,
    verbose: bool,
    description: Option<String>,
) -> AppResult<()> {
    if verbose {
        logger::init_console();
    }

    let url = parse_share_url(url)?;
    let app = AppContext::new(data_dir).await?;
    let db = app.runtime_inputs().db;
    ensure_db_migrated(&db).await?;
    let config = config::Manager::try_from(data_dir.trim())?;
    let share_crawler = bootstrap::services::build_share_crawler(&config);
    let (mut import_service, mut metadata_lookup) =
        bootstrap::services::build_import_service(&config);
    let file_index_service = bootstrap::services::build_file_index_service(db.clone());

    let share_url = application::import::ShareUrl::from(&url).ok_or_else(|| {
        error::AppError::InvalidParameter(format!(
            "unsupported share url '{url}', expected pan123, pan189, pan115, or quark share link"
        ))
    })?;

    // Fetch raw files once
    let raw_files = match share_crawler.raw_files_from_share_url(&share_url).await {
        Ok(files) => files,
        Err(err) => {
            eprintln!("Warning: failed to fetch raw files for indexing: {err}");
            vec![]
        }
    };

    // Index: reuse raw files
    if !raw_files.is_empty() {
        let seen: Vec<application::file_index::SeenFile> = raw_files
            .iter()
            .map(application::file_index::SeenFile::from_raw_file)
            .collect();
        if let Err(err) = file_index_service
            .record_seen_files(seen, description)
            .await
        {
            eprintln!("Warning: failed to index share url: {err}");
        }
    }

    // Import: reuse raw files
    let media_files = metadata_lookup.build_media_files(raw_files);
    let imported = import_service.transfer_media_files(&media_files).await?;
    let summaries = format_import_summaries(&imported);
    let verbose_notes = if verbose {
        format_verbose_import_notes(&imported)
    } else {
        Vec::new()
    };

    if summaries.is_empty() {
        println!("{NO_NEW_MEDIA_MESSAGE}");
    } else {
        for summary in summaries {
            println!("{summary}");
        }
    }

    for note in verbose_notes {
        println!("{note}");
    }

    if verbose && imported.is_empty() {
        println!(
            "详细信息: 本次没有生成任何导入结果，常见原因包括分享中没有可识别媒体、TMDB 未匹配到条目，或电影资源在入库前就被判定为已存在且无需覆盖。"
        );
    }

    Ok(())
}

async fn run_search_files(data_dir: &str, keyword: &str, limit: u64) -> AppResult<()> {
    let app = AppContext::new(data_dir).await?;
    let db = app.runtime_inputs().db;
    ensure_db_migrated(&db).await?;
    let service = bootstrap::services::build_file_index_service(db);
    let results = service.search_files(keyword, limit).await?;
    if results.is_empty() {
        println!("未找到匹配文件");
        return Ok(());
    }

    for (index, record) in results.iter().enumerate() {
        println!("{}. {}", index + 1, record.file_name);
        println!("   path: {}", record.file_path);
        println!("   size: {}", format_file_size(record.size));
        if let Some(md5) = &record.md5 {
            println!("   md5: {md5}");
        }
        if let Some(sha1) = &record.sha1 {
            println!("   sha1: {sha1}");
        }
        for description in record.descriptions.iter().take(3) {
            println!("   description: {description}");
        }
    }

    Ok(())
}

async fn ensure_db_migrated(db: &DatabaseConnection) -> AppResult<()> {
    Migrator::up(db, None)
        .await
        .map_err(|err| error::AppError::Runtime(format!("failed to run migration: {err}")))
}

fn format_file_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = size as f64;
    let mut unit_index = 0;

    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size} B")
    } else {
        format!("{value:.2} {} ({size} bytes)", UNITS[unit_index])
    }
}

fn parse_share_url(raw_url: &str) -> AppResult<Url> {
    let url = Url::parse(raw_url).map_err(|err| {
        error::AppError::InvalidParameter(format!("invalid share url '{raw_url}': {err}"))
    })?;

    application::import::ShareUrl::from(&url).ok_or_else(|| {
        error::AppError::InvalidParameter(format!(
            "unsupported share url '{raw_url}', expected pan123, pan189, pan115, or quark share link"
        ))
    })?;

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::{format_file_size, parse_share_url};

    #[test]
    fn parse_share_url_accepts_supported_provider() {
        let share_url = parse_share_url("https://www.123pan.com/s/test?pwd=pass").unwrap();

        assert_eq!(share_url.as_str(), "https://www.123pan.com/s/test?pwd=pass");
    }

    #[test]
    fn parse_share_url_rejects_unsupported_provider() {
        let err = parse_share_url("https://example.com/s/test").unwrap_err();

        assert_eq!(err.kind(), crate::error::AppErrorKind::InvalidParameter);
        assert!(err.to_string().contains("unsupported share url"));
    }

    #[test]
    fn format_file_size_keeps_bytes_and_adds_readable_unit() {
        assert_eq!(
            format_file_size(6_517_230_688),
            "6.07 GiB (6517230688 bytes)"
        );
        assert_eq!(format_file_size(512), "512 B");
    }
}
