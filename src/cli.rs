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
    Push(PushArgs),
}

#[derive(Args)]
pub struct DataDirArgs {
    /// data directory
    #[arg(short, long, default_value_t = String::from("./data"))]
    pub data_dir: String,
}

#[derive(Args)]
pub struct PushArgs {
    /// data directory
    #[arg(short, long, default_value_t = String::from("./data"))]
    pub data_dir: String,

    #[arg(short, long)]
    pub message: String,
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
