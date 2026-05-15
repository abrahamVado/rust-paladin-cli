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

    /// Optional fallback Ollama model if the primary one fails.
    #[arg(long, env = "PALADIN_FALLBACK_MODEL")]
    pub fallback_model: Option<String>,

    /// Ollama base URL.
    #[arg(
        long,
        env = "PALADIN_OLLAMA_URL",
        default_value = "http://localhost:11434"
    )]
    pub ollama_url: String,

    /// Maximum diff characters sent to the model.
    #[arg(long, default_value_t = 20_000)]
    pub max_diff_chars: usize,

    /// Maximum number of commit suggestions in a generated plan.
    #[arg(long, default_value_t = 5)]
    pub max_commits: usize,

    /// Maximum changed files sent in a single model request.
    #[arg(long, default_value_t = 6)]
    pub max_files_per_batch: usize,

    /// Maximum diff characters sent in a single model request batch.
    #[arg(long, default_value_t = 8_000)]
    pub max_batch_chars: usize,

    /// Retry attempts when the model returns invalid JSON.
    #[arg(long, default_value_t = 2)]
    pub retries: usize,

    /// Disable the interactive terminal preview.
    #[arg(long)]
    pub no_tui: bool,
}
