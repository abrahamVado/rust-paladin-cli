use crate::commit::schema::CommitSuggestion;
use colored::*;

pub fn print_commit_suggestion(suggestion: &CommitSuggestion, diff_truncated: bool) {
    println!();
    println!("{}", "Suggested commit:".bold());
    println!("{}", suggestion.commit_message().green().bold());

    if !suggestion.body.is_empty() {
        println!();
        println!("{}", "Body:".bold());
        for line in &suggestion.body {
            println!("- {}", line.trim().trim_start_matches("- "));
        }
    }

    println!();
    println!("{} {}", "Risk:".bold(), color_risk(&suggestion.risk));

    if diff_truncated {
        println!();
        println!(
            "{}",
            "Note: diff was truncated before being sent to the model.".yellow()
        );
    }
}

fn color_risk(value: &str) -> colored::ColoredString {
    match value {
        "low" => value.green(),
        "medium" => value.yellow(),
        "high" => value.red().bold(),
        _ => value.normal(),
    }
}
