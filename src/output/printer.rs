use crate::commit::schema::CommitPlan;
use colored::*;

pub fn print_commit_plan(plan: &CommitPlan, diff_truncated: bool) {
    println!();
    println!("{}", "Suggested commit plan:".bold());

    if !plan.strategy.trim().is_empty() {
        println!("{}", plan.strategy.trim().italic());
    }

    for (index, suggestion) in plan.commits.iter().enumerate() {
        println!();
        println!("{}", format!("Commit {}", index + 1).bold());
        println!("{}", suggestion.commit_message().green().bold());

        if !suggestion.body.is_empty() {
            println!("{}", "Body:".bold());
            for line in &suggestion.body {
                println!("- {}", line.trim().trim_start_matches("- "));
            }
        }

        println!("{} {}", "Risk:".bold(), color_risk(&suggestion.risk));
        println!("{}", "Files:".bold());
        for path in &suggestion.files {
            println!("- {}", path);
        }
    }

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
