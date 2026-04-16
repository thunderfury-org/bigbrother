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
}

#[derive(Args)]
pub struct DataDirArgs {
    /// data directory
    #[arg(short, long, default_value_t = String::from("./data"))]
    pub data_dir: String,
}

#[derive(Args)]
pub struct ImportShareUrlArgs {
    #[command(flatten)]
    pub data_dir: DataDirArgs,
    #[arg(short, long)]
    pub verbose: bool,
    pub url: String,
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
}
