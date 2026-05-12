use migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;
use url::Url;

use crate::{
    application::{self, import::MetadataLookup, share_crawler::ShareCrawler},
    config,
    error::{self, AppResult},
    infrastructure::{
        client,
        import::gateway::{PanLibraryGateway, ShareImportGateway, TmdbMetadataGateway},
        import::local_store::FilesystemImportLocalStore,
        repo::file_index::SeaOrmFileIndexRepository,
        services::{FileIndexRuntimeService, ImportService},
    },
    logger,
};

use crate::interface::import::{
    NO_NEW_MEDIA_MESSAGE, format_import_summaries, format_verbose_import_notes,
};

pub(crate) async fn run_import_share_url(
    data_dir: &str,
    url: &str,
    verbose: bool,
    description: Option<String>,
) -> AppResult<()> {
    if verbose {
        logger::init_console();
    }

    let url = parse_share_url(url)?;
    let db = connect_db(data_dir).await?;
    let config = config::Manager::try_from(data_dir.trim())?;

    let pan115 = client::pan115::Client::new();
    let pan123 = client::pan123::Client::new(
        &config.get_pan123_config().passport,
        &config.get_pan123_config().password,
        &format!("{}/pan123", config.get_cache_dir()),
    );
    let pan189 = client::pan189::Client::new(client::pan189::AuthConfig {
        username: config.get_pan189_config().username.clone(),
        password: config.get_pan189_config().password.clone(),
        cache_dir: format!("{}/pan189", config.get_cache_dir()),
    });
    let quark = client::quark::Client::new(&config.get_quark_config().cookie);
    let tmdb = client::tmdb::Client::new(&config.get_tmdb_config().api_key);

    let share_crawler = ShareCrawler::new(ShareImportGateway::new(
        pan115,
        pan123.clone(),
        pan189,
        quark,
    ));

    let mut import_service = ImportService::new(
        PanLibraryGateway::new(pan123),
        TmdbMetadataGateway::new(tmdb),
        FilesystemImportLocalStore::new(
            config.get_library_config().remote_path.clone(),
            config.get_library_config().local_path.clone(),
            config.get_media_server_config().get_strm_download_url(),
        ),
    );
    let mut metadata_lookup = MetadataLookup::default();

    let file_index_service =
        FileIndexRuntimeService::new(SeaOrmFileIndexRepository::new(db.clone()));

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

pub(crate) async fn run_search_files(data_dir: &str, keyword: &str, limit: u64) -> AppResult<()> {
    let db = connect_db(data_dir).await?;
    let service = FileIndexRuntimeService::new(SeaOrmFileIndexRepository::new(db));
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

async fn connect_db(data_dir: &str) -> AppResult<DatabaseConnection> {
    let config = config::Manager::try_from(data_dir.trim())?;
    let db_dir = config.get_db_dir();
    if !std::fs::exists(db_dir.as_str())? {
        std::fs::create_dir_all(db_dir.as_str())?;
    }
    let conn_str = format!("sqlite:{}/data.db?mode=rwc", db_dir);
    let mut opt = sea_orm::ConnectOptions::new(conn_str);
    opt.sqlx_logging(false);
    let db = sea_orm::Database::connect(opt)
        .await
        .map_err(|err| error::AppError::Database(format!("failed to connect database: {err}"), false))?;

    Migrator::up(&db, None)
        .await
        .map_err(|err| error::AppError::Database(format!("failed to run migration: {err}"), false))?;

    Ok(db)
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

        assert!(matches!(err, crate::error::AppError::InvalidParameter(_)));
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
