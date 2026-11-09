use crate::cli::{Cli, Cmd};
use clap::Parser;
use daemon::ota::UpdateOpts;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match args.command {
        Some(Cmd::Update { image }) => daemon::ota::update(UpdateOpts { image }).await,
        None => daemon::run(args.into()).await,
    }
}
