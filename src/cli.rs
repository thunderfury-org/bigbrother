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
    Once(DataDirArgs),
}

#[derive(Args)]
pub struct DataDirArgs {
    /// data directory
    #[arg(short, long, default_value_t = String::from("./data"))]
    pub data_dir: String,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert()
    }
}
