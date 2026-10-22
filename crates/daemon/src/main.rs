use crate::cli::Cli;
use clap::Parser;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    daemon::run(args.into()).await
}