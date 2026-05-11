pub mod prompt;
pub mod schema;

use crate::cli::CommitArgs;
use crate::git;
use crate::llm::ollama::OllamaClient;
use crate::output;
use anyhow::{anyhow, Context, Result};
use schema::CommitSuggestion;
use std::io::{self, Write};

pub async fn run(args: CommitArgs) -> Result<()> {
    let changes = git::collect_changes(args.staged, args.max_diff_chars)?;

    if !git::has_changes(&changes) {
        return Err(anyhow!(
            "no {}changes found",
            if args.staged { "staged " } else { "" }
        ));
    }

    let prompt = prompt::build_commit_prompt(&changes)?;
    let client = OllamaClient::new(args.ollama_url.clone(), args.model.clone());

    println!("Analyzing Git diff with {}...", args.model);

    let raw_response = client.generate_json(&prompt).await?;
    let suggestion: CommitSuggestion = serde_json::from_str(&raw_response)
        .with_context(|| format!("model did not return valid commit JSON:\n{}", raw_response))?;

    suggestion.validate()?;

    output::print_commit_suggestion(&suggestion, changes.diff_truncated);

    if !suggestion.should_commit {
        return Err(anyhow!(
            "model marked this change as not safe to commit automatically"
        ));
    }

    if !args.yes && !confirm("Create this commit? [y/N] ")? {
        println!("Commit cancelled.");
        return Ok(());
    }

    if !args.staged {
        git::stage_all()?;
    }

    git::commit(&suggestion.commit_message(), &suggestion.body)?;

    println!("Commit created.");
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read confirmation")?;

    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}
