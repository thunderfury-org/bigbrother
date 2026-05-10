mod handler;

use crate::error::AppResult;
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
    ImportShareUrl(ImportShareUrlArgs),
    SearchFiles(SearchFilesArgs),
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
pub struct SearchFilesArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long, default_value_t = 20)]
    pub limit: u64,
    pub keyword: String,
}

pub async fn run(cli: Cli) -> AppResult<()> {
    match &cli.command {
        Commands::Server(_) => unreachable!("server command is handled by main"),
        Commands::ImportShareUrl(args) => {
            handler::run_import_share_url(
                args.data_dir.data_dir.as_str(),
                &args.url,
                args.verbose,
                args.description.clone(),
            )
            .await
        }
        Commands::SearchFiles(args) => {
            handler::run_search_files(args.data_dir.data_dir.as_str(), &args.keyword, args.limit)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::CommandFactory;
    use clap::Parser;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert()
    }

    #[test]
    fn parses_import_share_url_command() {
        let cli = Cli::parse_from([
            "bigbrother",
            "import-share-url",
            "--verbose",
            "--data-dir",
            "./data",
            "https://www.123pan.com/s/test?pwd=pass",
        ]);

        match cli.command {
            Commands::ImportShareUrl(args) => {
                assert_eq!(args.data_dir.data_dir, "./data");
                assert!(args.verbose);
                assert_eq!(args.url, "https://www.123pan.com/s/test?pwd=pass");
            }
            _ => panic!("expected import-share-url command"),
        }
    }

    #[test]
    fn parses_import_share_url_description() {
        let cli = Cli::parse_from([
            "bigbrother",
            "import-share-url",
            "--description",
            "from cli",
            "--data-dir",
            "./data",
            "https://www.123pan.com/s/test?pwd=pass",
        ]);

        match cli.command {
            Commands::ImportShareUrl(args) => {
                assert_eq!(args.description.as_deref(), Some("from cli"));
            }
            _ => panic!("expected import-share-url command"),
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
}
