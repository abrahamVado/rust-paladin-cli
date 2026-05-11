mod cli;
mod commit;
mod git;
mod llm;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Commit(args) => {
            commit::run(args).await?;
        }
    }

    Ok(())
}
