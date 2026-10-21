use crate::cli::Cli;
use clap::Parser;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_max_level(args.verbosity)
        .init();

    daemon::run().await
}