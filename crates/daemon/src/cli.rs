use clap::Parser;
use clap_verbosity_flag::InfoLevel;

#[derive(Debug, Parser)]
pub struct Cli {
    #[command(flatten)]
    pub verbosity: clap_verbosity_flag::Verbosity<InfoLevel>,
}
