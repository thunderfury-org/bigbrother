mod config;
mod context;
mod handler;
mod logger;
pub(crate) mod server;
mod telegram_export;

use crate::migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;

use crate::error::{AppError, AppResult};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Server(DataDirArgs),
    Share(ShareArgs),
    SearchFiles(SearchFilesArgs),
    TelegramExport(TelegramExportArgs),
}

#[derive(Args)]
pub struct ShareArgs {
    #[command(subcommand)]
    pub command: ShareCommands,
}

#[derive(Args)]
pub struct TelegramExportArgs {
    #[command(subcommand)]
    pub command: TelegramExportCommands,
}

#[derive(Subcommand)]
pub enum ShareCommands {
    /// 列出分享链接中的文件
    List(ShareListArgs),
    /// 导入分享链接中的媒体文件
    Import(ImportShareUrlArgs),
    /// 解析分享链接中的文件名并查询 TMDB 信息
    Parse(ParseArgs),
}

#[derive(Subcommand)]
pub enum TelegramExportCommands {
    /// 从 Telegram Desktop 导出文件建立文件索引
    Index(TelegramExportIndexArgs),
}

#[derive(Args)]
pub struct DataDirArgs {
    /// data directory
    #[arg(short = 'D', long, default_value_t = String::from("./data"))]
    pub data_dir: String,
}

#[derive(Args)]
pub struct ImportShareUrlArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    pub url: String,
}

#[derive(Args)]
pub struct ShareListArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    pub url: String,
}

#[derive(Args)]
pub struct ParseArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    pub url: String,
}

#[derive(Args)]
pub struct SearchFilesArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long, default_value_t = 20)]
    pub limit: u64,
    pub keyword: String,
}

#[derive(Args)]
pub struct TelegramExportIndexArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long)]
    pub input: String,
    #[arg(short, long)]
    pub verbose: bool,
    #[arg(long, default_value_t = 300)]
    pub delay_ms: u64,
    #[arg(long)]
    pub retry_all: bool,
}

pub async fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        Commands::Server(args) => server::run(&args.data_dir).await,
        Commands::Share(args) => match args.command {
            ShareCommands::List(args) => {
                handler::run_share_list(args.data_dir.data_dir.as_str(), &args.url).await
            }
            ShareCommands::Import(args) => {
                handler::run_import_share_url(
                    args.data_dir.data_dir.as_str(),
                    &args.url,
                    args.verbose,
                    args.description,
                )
                .await
            }
            ShareCommands::Parse(args) => {
                handler::run_share_parse(
                    args.data_dir.data_dir.as_str(),
                    &args.url,
                    args.description,
                )
                .await
            }
        },
        Commands::SearchFiles(args) => {
            handler::run_search_files(args.data_dir.data_dir.as_str(), &args.keyword, args.limit)
                .await
        }
        Commands::TelegramExport(args) => match args.command {
            TelegramExportCommands::Index(args) => {
                handler::run_telegram_export_index(
                    args.data_dir.data_dir.as_str(),
                    args.input.as_str(),
                    args.verbose,
                    args.delay_ms,
                    args.retry_all,
                )
                .await
            }
        },
    }
}

async fn connect_db(db_dir: &str) -> AppResult<DatabaseConnection> {
    if !std::fs::exists(db_dir)? {
        std::fs::create_dir_all(db_dir)?;
    }
    let conn_str = format!("sqlite:{db_dir}/data.db?mode=rwc");
    let mut opt = sea_orm::ConnectOptions::new(conn_str);
    opt.sqlx_logging(false);
    let db = sea_orm::Database::connect(opt)
        .await
        .map_err(|err| AppError::Database(format!("failed to connect database: {err}"), false))?;

    Migrator::up(&db, None)
        .await
        .map_err(|err| AppError::Database(format!("failed to run migration: {err}"), false))?;
    crate::infrastructure::entity::file_index::backfill_file_location_fts(&db).await?;

    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, ShareCommands, TelegramExportCommands, context::CliContext};
    use clap::CommandFactory;
    use clap::Parser;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempCliDataDir {
        path: PathBuf,
    }

    impl TempCliDataDir {
        fn new() -> Self {
            let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("bigbrother-cli-{}-{counter}", std::process::id()));
            fs::create_dir_all(path.join("config")).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_config(&self, config: &str) {
            fs::write(self.path.join("config/config.yaml"), config).unwrap();
        }
    }

    impl Drop for TempCliDataDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert()
    }

    #[test]
    fn parses_share_import_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "share",
            "import",
            "--verbose",
            "--data-dir",
            "./data",
            "https://www.123pan.com/s/test?pwd=pass",
        ]);

        match cli.command {
            Commands::Share(args) => match args.command {
                ShareCommands::Import(args) => {
                    assert_eq!(args.data_dir.data_dir, "./data");
                    assert!(args.verbose);
                    assert_eq!(args.url, "https://www.123pan.com/s/test?pwd=pass");
                }
                _ => panic!("expected share import command"),
            },
            _ => panic!("expected share command"),
        }
    }

    #[test]
    fn parses_share_import_description() {
        let cli = Cli::parse_from([
            "bigbrother",
            "share",
            "import",
            "--description",
            "from cli",
            "--data-dir",
            "./data",
            "https://www.123pan.com/s/test?pwd=pass",
        ]);

        match cli.command {
            Commands::Share(args) => match args.command {
                ShareCommands::Import(args) => {
                    assert_eq!(args.description.as_deref(), Some("from cli"));
                }
                _ => panic!("expected share import command"),
            },
            _ => panic!("expected share command"),
        }
    }

    #[test]
    fn parses_share_list_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "share",
            "list",
            "--data-dir",
            "./data",
            "https://115.com/s/test?rc=pass",
        ]);

        match cli.command {
            Commands::Share(args) => match args.command {
                ShareCommands::List(args) => {
                    assert_eq!(args.data_dir.data_dir, "./data");
                    assert_eq!(args.url, "https://115.com/s/test?rc=pass");
                }
                _ => panic!("expected share list command"),
            },
            _ => panic!("expected share command"),
        }
    }

    #[test]
    fn parses_share_parse_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "share",
            "parse",
            "--description",
            "test desc",
            "--data-dir",
            "./data",
            "https://115.com/s/test?rc=pass",
        ]);

        match cli.command {
            Commands::Share(args) => match args.command {
                ShareCommands::Parse(args) => {
                    assert_eq!(args.data_dir.data_dir, "./data");
                    assert_eq!(args.description.as_deref(), Some("test desc"));
                    assert_eq!(args.url, "https://115.com/s/test?rc=pass");
                }
                _ => panic!("expected share parse command"),
            },
            _ => panic!("expected share command"),
        }
    }

    #[test]
    fn parses_search_files_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "search-files",
            "--limit",
            "50",
            "--data-dir",
            "./data",
            "movie",
        ]);

        match cli.command {
            Commands::SearchFiles(args) => {
                assert_eq!(args.keyword, "movie");
                assert_eq!(args.limit, 50);
            }
            _ => panic!("expected search-files command"),
        }
    }

    #[test]
    fn parses_telegram_export_index_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "telegram-export",
            "index",
            "--data-dir",
            "./data",
            "--input",
            "/tmp/result.json",
            "--verbose",
            "--delay-ms",
            "250",
            "--retry-all",
        ]);

        match cli.command {
            Commands::TelegramExport(args) => match args.command {
                TelegramExportCommands::Index(args) => {
                    assert_eq!(args.data_dir.data_dir, "./data");
                    assert_eq!(args.input, "/tmp/result.json");
                    assert!(args.verbose);
                    assert_eq!(args.delay_ms, 250);
                    assert!(args.retry_all);
                }
            },
            _ => panic!("expected telegram-export command"),
        }
    }

    #[test]
    fn telegram_export_index_uses_default_delay_ms() {
        let cli = Cli::parse_from([
            "bigbrother",
            "telegram-export",
            "index",
            "--data-dir",
            "./data",
            "--input",
            "/tmp/result.json",
        ]);

        match cli.command {
            Commands::TelegramExport(args) => match args.command {
                TelegramExportCommands::Index(args) => {
                    assert_eq!(args.delay_ms, 300);
                    assert!(!args.retry_all);
                    assert!(!args.verbose);
                }
            },
            _ => panic!("expected telegram-export command"),
        }
    }

    #[tokio::test]
    async fn cli_context_initializes_and_reuses_db_connection() {
        let data_dir = TempCliDataDir::new();
        let ctx = CliContext::new(data_dir.path().to_str().unwrap()).unwrap();

        let first = ctx.db().await.unwrap().clone();
        let second = ctx.db().await.unwrap().clone();

        assert!(
            std::path::Path::new(&format!("{}/db/data.db", data_dir.path().display())).exists()
        );
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
    }

    #[tokio::test]
    async fn cli_context_passes_pan115_request_interval_to_client() {
        let data_dir = TempCliDataDir::new();
        data_dir.write_config(
            r#"
pan115:
  request_interval_ms: 900
"#,
        );

        let ctx = CliContext::new(data_dir.path().to_str().unwrap()).unwrap();

        assert_eq!(ctx.pan115().min_request_interval().await.as_millis(), 900);
    }
}
