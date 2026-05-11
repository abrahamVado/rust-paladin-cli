use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "paladin")]
#[command(author, version, about = "Local Gemma-powered Git commit assistant")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Analyze the current Git diff and create a conventional commit.
    Commit(CommitArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CommitArgs {
    /// Only analyze already-staged changes.
    #[arg(long)]
    pub staged: bool,

    /// Skip confirmation and create the commit directly.
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// Ollama model name.
    #[arg(long, env = "PALADIN_MODEL", default_value = "gemma4:e4b")]
    pub model: String,

    /// Ollama base URL.
    #[arg(long, env = "PALADIN_OLLAMA_URL", default_value = "http://localhost:11434")]
    pub ollama_url: String,

    /// Maximum diff characters sent to the model.
    #[arg(long, default_value_t = 60_000)]
    pub max_diff_chars: usize,
}
