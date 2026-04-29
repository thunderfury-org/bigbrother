use bootstrap::{AppContext, AppRuntime};
use clap::Parser;
use error::AppResult;
use interface::{
    cli::{Cli, Commands},
    import::{NO_NEW_MEDIA_MESSAGE, format_import_summaries, format_verbose_import_notes},
};
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
            if let Err(err) =
                run_import_share_url(args.data_dir.data_dir.as_str(), &args.url, args.verbose).await
            {
                eprintln!("Failed to import from share url: {err}");
                std::process::exit(1);
            }
        }
    }
}

async fn run_server(data_dir: &str) -> AppResult<()> {
    let app = AppContext::new(data_dir).await?;
    AppRuntime::from_app(app)?.run().await
}

async fn run_import_share_url(data_dir: &str, url: &str, verbose: bool) -> AppResult<()> {
    if verbose {
        logger::init_console();
    }

    let url = parse_share_url(url)?;
    let config = config::Manager::try_from(data_dir.trim())?;
    let import_service = bootstrap::services::build_import_service(&config);
    let share_url = application::import::ShareUrl::from(&url).ok_or_else(|| {
        error::AppError::InvalidParameter(format!(
            "unsupported share url '{url}', expected pan123, pan189, or pan115 share link"
        ))
    })?;
    let imported = import_service.import_from_share_url(&share_url).await?;
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

fn parse_share_url(raw_url: &str) -> AppResult<Url> {
    let url = Url::parse(raw_url).map_err(|err| {
        error::AppError::InvalidParameter(format!("invalid share url '{raw_url}': {err}"))
    })?;

    application::import::ShareUrl::from(&url).ok_or_else(|| {
        error::AppError::InvalidParameter(format!(
            "unsupported share url '{raw_url}', expected pan123, pan189, or pan115 share link"
        ))
    })?;

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::parse_share_url;

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
}
