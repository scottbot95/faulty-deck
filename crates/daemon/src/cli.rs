use std::path::PathBuf;
use clap::Parser;
use clap_verbosity_flag::InfoLevel;
use etcetera::AppStrategy;
use daemon::{Opts, ETCETERA};

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(flatten)]
    pub verbosity: clap_verbosity_flag::Verbosity<InfoLevel>,

    #[arg(short, long)]
    pub config_file: Option<PathBuf>,
}

impl From<Cli> for Opts {
    fn from(value: Cli) -> Self {
        let config_path = value.config_file
            .clone()
            .unwrap_or_else(|| {
                ETCETERA.in_config_dir("config.toml")
            });

        Self {
            config_path,
        }
    }
}
