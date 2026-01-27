//! bird - A fast X/Twitter CLI for reading tweets

use bird::cli::Cli;
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    cli.run().await
}
