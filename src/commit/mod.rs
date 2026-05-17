pub mod prompt;
pub mod schema;

use crate::cli::CommitArgs;
use crate::git;
use crate::llm::ollama::{OllamaClient, OllamaGenerateError};
use crate::output;
use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use schema::{CommitPlan, CommitSuggestion};
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;
use serde_json::{Map, Value};

pub async fn run(args: CommitArgs) -> Result<()> {
    let changes = git::collect_changes(args.staged, args.max_diff_chars)?;

    if !git::has_changes(&changes) {
        return Err(anyhow!(
            "no {}changes found",
            if args.staged { "staged " } else { "" }
        ));
    }

    println!("Analyzing Git diff with {}...", args.model);

    let plan = generate_batched_commit_plan(&args, &changes).await?;

    if args.no_tui {
        output::print_commit_plan(&plan, changes.diff_truncated);
    } else if !io::stdout().is_terminal() {
        println!("Interactive preview unavailable because stdout is not a terminal.");
        output::print_commit_plan(&plan, changes.diff_truncated);
    } else {
        println!("Opening interactive commit preview...");
        println!("Use arrow keys to switch commits, then press Enter to continue.");

        if let Err(error) = preview_commit_plan_tui(&plan, changes.diff_truncated) {
            println!("Interactive preview unavailable: {error}");
            println!("Falling back to plain-text preview.");
        }

        println!();
        println!("Commit preview summary:");
        output::print_commit_plan(&plan, changes.diff_truncated);
    }

    if !args.yes && !confirm("Create these commit(s)? [y/N] ")? {
        println!("Commit cancelled.");
        return Ok(());
    }

    apply_commit_plan(&plan)?;

    println!("Commit plan created.");
    Ok(())
}

async fn generate_commit_suggestion(
    args: &CommitArgs,
    prompt: &str,
    batch_files: &[String],
) -> Result<CommitSuggestion> {
    let primary = OllamaClient::new(args.ollama_url.clone(), args.model.clone());

    match generate_single_commit(&primary, prompt, batch_files, args.retries).await {
        Ok(plan) => Ok(plan),
        Err(primary_error) => {
            let Some(fallback_model) = args.fallback_model.clone() else {
                return Err(primary_error).with_context(|| testing_hint(&args.model, None));
            };

            let is_load_error = primary_error
                .downcast_ref::<OllamaGenerateError>()
                .is_some_and(|error| matches!(error, OllamaGenerateError::ModelLoadFailure { .. }));

            if !is_load_error {
                return Err(primary_error)
                    .with_context(|| testing_hint(&args.model, Some(&fallback_model)));
            }

            println!(
                "Primary model `{}` failed to load. Retrying with fallback model `{}`...",
                args.model, fallback_model
            );

            let fallback = OllamaClient::new(args.ollama_url.clone(), fallback_model.clone());
            generate_single_commit(&fallback, prompt, batch_files, args.retries)
                .await
                .with_context(|| testing_hint(&args.model, Some(&fallback_model)))
        }
    }
}

async fn generate_single_commit(
    client: &OllamaClient,
    prompt: &str,
    batch_files: &[String],
    retries: usize,
) -> Result<CommitSuggestion> {
    let mut attempt = 0usize;
    let mut current_prompt = prompt.to_string();

    loop {
        let raw_response = client.generate_json(&current_prompt).await?;

        match parse_commit_suggestion(&raw_response, batch_files) {
            Ok(commit) => return Ok(commit),
            Err(error) => {
                if attempt >= retries {
                    return Err(error).with_context(|| {
                        format!(
                            "model did not return a valid commit suggestion after {} attempt(s)\nlast response:\n{}",
                            attempt + 1,
                            raw_response
                        )
                    });
                }

                attempt += 1;
                let last_error = error.to_string();
                current_prompt = format!(
                    "{prompt}\n\nYour previous reply was invalid.\nValidation error: {last_error}\nFix your previous reply by returning exactly one JSON object matching the required schema.\nDo not explain the mistake.\nDo not ask questions.\nDo not return markdown.\nDo not return tool calls.\nIf your previous reply used an `error` field, discard it and infer the best commit suggestion from the diff.\nPrevious invalid reply:\n{raw_response}"
                );
            }
        }
    }
}

async fn generate_batched_commit_plan(
    args: &CommitArgs,
    changes: &git::GitChangeSet,
) -> Result<CommitPlan> {
    let batches = build_file_batches(
        &changes.file_diffs,
        args.max_files_per_batch.max(1),
        args.max_batch_chars.max(500),
    );

    if batches.is_empty() {
        return Err(anyhow!("no file batches available to generate commits"));
    }

    let capped_batches = batches
        .into_iter()
        .take(args.max_commits.max(1))
        .collect::<Vec<_>>();
    let total_batches = capped_batches.len();
    let mut commits = Vec::with_capacity(total_batches);

    for (index, batch) in capped_batches.iter().enumerate() {
        let prompt = prompt::build_batch_commit_prompt(batch, args.max_file_diff_chars);
        let batch_files = batch
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        println!(
            "Generating commit {}/{} for {} file(s)...",
            index + 1,
            total_batches,
            batch_files.len()
        );
        let mut commit = generate_commit_suggestion(args, &prompt, &batch_files).await?;
        commit.files = batch_files;
        commits.push(commit);
    }

    let initial_plan = CommitPlan::from_commits(
        format!(
            "Generated {} smaller commit batch(es) locally to keep model requests short.",
            commits.len()
        ),
        commits,
    );

    refine_commit_plan(args, initial_plan).await
}

async fn refine_commit_plan(args: &CommitArgs, initial_plan: CommitPlan) -> Result<CommitPlan> {
    if initial_plan.commits.len() <= 1 {
        return Ok(initial_plan);
    }

    println!(
        "Reviewing all {} commit part(s) together for a final plan...",
        initial_plan.commits.len()
    );

    let prompt =
        prompt::build_plan_review_prompt(&initial_plan.commits, args.max_files_per_batch);
    let allowed_files = initial_plan
        .commits
        .iter()
        .flat_map(|commit| commit.files.iter().cloned())
        .collect::<Vec<_>>();
    let primary = OllamaClient::new(args.ollama_url.clone(), args.model.clone());

    match generate_finalized_plan(&primary, &prompt, &allowed_files, args.retries).await {
        Ok(plan) => Ok(plan),
        Err(primary_error) => {
            let Some(fallback_model) = args.fallback_model.clone() else {
                return Ok(initial_plan);
            };

            let is_load_error = primary_error
                .downcast_ref::<OllamaGenerateError>()
                .is_some_and(|error| matches!(error, OllamaGenerateError::ModelLoadFailure { .. }));

            if !is_load_error {
                return Ok(initial_plan);
            }

            println!(
                "Primary model `{}` failed to load during final plan review. Retrying with fallback model `{}`...",
                args.model, fallback_model
            );

            let fallback = OllamaClient::new(args.ollama_url.clone(), fallback_model);
            generate_finalized_plan(&fallback, &prompt, &allowed_files, args.retries)
                .await
                .or(Ok(initial_plan))
        }
    }
}

async fn generate_finalized_plan(
    client: &OllamaClient,
    prompt: &str,
    allowed_files: &[String],
    retries: usize,
) -> Result<CommitPlan> {
    let mut attempt = 0usize;
    let mut current_prompt = prompt.to_string();

    loop {
        let raw_response = client.generate_json(&current_prompt).await?;

        match parse_commit_plan(&raw_response, allowed_files) {
            Ok(plan) => return Ok(plan),
            Err(error) => {
                if attempt >= retries {
                    return Err(error).with_context(|| {
                        format!(
                            "model did not return a valid final commit plan after {} attempt(s)\nlast response:\n{}",
                            attempt + 1,
                            raw_response
                        )
                    });
                }

                attempt += 1;
                let last_error = error.to_string();
                current_prompt = format!(
                    "{prompt}\n\nYour previous reply was invalid.\nValidation error: {last_error}\nFix your previous reply by returning exactly one JSON object matching the required schema.\nDo not explain the mistake.\nDo not ask questions.\nDo not return markdown.\nDo not return tool calls.\nEvery allowed file must appear exactly once.\nPrevious invalid reply:\n{raw_response}"
                );
            }
        }
    }
}

fn parse_commit_suggestion(raw_response: &str, batch_files: &[String]) -> Result<CommitSuggestion> {
    let extracted = extract_json_block(raw_response).unwrap_or(raw_response);
    let mut suggestion = parse_commit_suggestion_value(extracted)
        .or_else(|_| parse_commit_suggestion_value(raw_response))
        .with_context(|| format!("failed to parse commit suggestion JSON:\n{}", raw_response))?;
    suggestion.files = batch_files.to_vec();
    suggestion.validate()?;
    Ok(suggestion)
}

fn parse_commit_plan(raw_response: &str, allowed_files: &[String]) -> Result<CommitPlan> {
    let extracted = extract_json_block(raw_response).unwrap_or(raw_response);
    let plan = serde_json::from_str::<CommitPlan>(extracted)
        .or_else(|_| serde_json::from_str::<CommitPlan>(raw_response))
        .with_context(|| format!("failed to parse final commit plan JSON:\n{}", raw_response))?;
    plan.validate()?;
    validate_plan_files(&plan, allowed_files)?;
    Ok(plan)
}

fn validate_plan_files(plan: &CommitPlan, allowed_files: &[String]) -> Result<()> {
    let expected = allowed_files.iter().cloned().collect::<HashSet<_>>();
    let actual = plan
        .commits
        .iter()
        .flat_map(|commit| commit.files.iter().cloned())
        .collect::<HashSet<_>>();

    if actual != expected {
        let missing = expected
            .difference(&actual)
            .cloned()
            .collect::<Vec<_>>();
        let unexpected = actual
            .difference(&expected)
            .cloned()
            .collect::<Vec<_>>();

        return Err(anyhow!(
            "final commit plan changed the file set; missing: [{}], unexpected: [{}]",
            missing.join(", "),
            unexpected.join(", ")
        ));
    }

    Ok(())
}

fn parse_commit_suggestion_value(value: &str) -> Result<CommitSuggestion> {
    let parsed: Value = serde_json::from_str(value)?;
    parse_commit_suggestion_json(&parsed)
}

fn parse_commit_suggestion_json(value: &Value) -> Result<CommitSuggestion> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("commit suggestion must be a JSON object"))?;

    if let Ok(suggestion) = serde_json::from_value::<CommitSuggestion>(value.clone()) {
        return Ok(suggestion);
    }

    if let Some(nested) = find_nested_suggestion(object) {
        if let Ok(suggestion) = serde_json::from_value::<CommitSuggestion>(nested.clone()) {
            return Ok(suggestion);
        }
    }

    normalize_commit_suggestion(object)
}

fn find_nested_suggestion(object: &Map<String, Value>) -> Option<&Value> {
    ["commit", "suggestion", "result", "output", "data"]
        .into_iter()
        .find_map(|key| object.get(key))
}

fn normalize_commit_suggestion(object: &Map<String, Value>) -> Result<CommitSuggestion> {
    let commit_type = read_string_alias(object, &["type", "commit_type", "kind"])
        .ok_or_else(|| anyhow!("missing required field `type`"))?;
    let subject = read_string_alias(object, &["subject", "title", "message", "summary"])
        .ok_or_else(|| anyhow!("missing required field `subject`"))?;
    let risk = read_string_alias(object, &["risk", "risk_level", "impact"])
        .unwrap_or_else(|| "medium".to_string());
    let scope = read_optional_string_alias(object, &["scope", "area"]);
    let body = read_body_alias(object, &["body", "details", "description", "bullets"]);

    Ok(CommitSuggestion {
        commit_type,
        scope,
        subject,
        body,
        risk,
        files: Vec::new(),
    })
}

fn read_string_alias(object: &Map<String, Value>, aliases: &[&str]) -> Option<String> {
    aliases.iter().find_map(|alias| {
        object.get(*alias).and_then(|value| match value {
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            _ => None,
        })
    })
}

fn read_optional_string_alias(object: &Map<String, Value>, aliases: &[&str]) -> Option<String> {
    aliases.iter().find_map(|alias| {
        object.get(*alias).and_then(|value| match value {
            Value::Null => None,
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            _ => None,
        })
    })
}

fn read_body_alias(object: &Map<String, Value>, aliases: &[&str]) -> Vec<String> {
    aliases
        .iter()
        .find_map(|alias| object.get(*alias))
        .map(|value| match value {
            Value::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    Value::String(text) => {
                        let trimmed = text.trim();
                        (!trimmed.is_empty()).then(|| trimmed.to_string())
                    }
                    _ => None,
                })
                .collect(),
            Value::String(text) => text
                .lines()
                .map(|line| line.trim().trim_start_matches("- ").to_string())
                .filter(|line| !line.is_empty())
                .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default()
}

fn extract_json_block(value: &str) -> Option<&str> {
    let start = value.find('{')?;
    let end = value.rfind('}')?;
    value.get(start..=end)
}

fn build_file_batches(
    files: &[git::FileDiff],
    max_files_per_batch: usize,
    max_batch_chars: usize,
) -> Vec<Vec<git::FileDiff>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chars = 0usize;

    for file in files {
        let file_chars = file.diff.chars().count().max(file.path.chars().count());
        let would_exceed_files = current.len() >= max_files_per_batch;
        let would_exceed_chars =
            !current.is_empty() && current_chars + file_chars > max_batch_chars;

        if would_exceed_files || would_exceed_chars {
            batches.push(current);
            current = Vec::new();
            current_chars = 0;
        }

        current_chars += file_chars.min(max_batch_chars);
        current.push(file.clone());
    }

    if !current.is_empty() {
        batches.push(current);
    }

    batches
}

fn testing_hint(primary_model: &str, fallback_model: Option<&str>) -> String {
    let mut hint = format!(
        "Testing tips:\n- Try a smaller or more reliable local model, for example `cargo run -- commit --model qwen2.5-coder:7b`\n- Verify Ollama can run the model directly with `ollama run {}`\n- If the model keeps ignoring the JSON schema, try another coding model",
        primary_model
    );

    if let Some(fallback) = fallback_model {
        hint.push_str(&format!(
            "\n- You can also set a fallback now: `cargo run -- commit --fallback-model {}`",
            fallback
        ));
    }

    hint
}

fn apply_commit_plan(plan: &CommitPlan) -> Result<()> {
    plan.validate()?;

    for commit in &plan.commits {
        git::add_paths(&commit.files)?;
        git::commit_paths(&commit.commit_message(), &commit.body, &commit.files)?;
    }

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

fn preview_commit_plan_tui(plan: &CommitPlan, diff_truncated: bool) -> Result<()> {
    if plan.commits.is_empty() {
        return Ok(());
    }

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal UI")?;
    let mut selected = 0usize;

    let result = loop {
        terminal.draw(|frame| render_plan(frame, plan, selected, diff_truncated))?;

        if event::poll(Duration::from_millis(200)).context("failed to poll terminal events")? {
            if let Event::Key(key) = event::read().context("failed to read terminal event")? {
                match key.code {
                    KeyCode::Left | KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Right | KeyCode::Down => {
                        selected = (selected + 1).min(plan.commits.len().saturating_sub(1));
                    }
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => break Ok(()),
                    _ => {}
                }
            }
        }
    };

    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to restore cursor")?;
    result
}

fn render_plan(frame: &mut Frame<'_>, plan: &CommitPlan, selected: usize, diff_truncated: bool) {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let header = Paragraph::new(format!(
        "Commit plan preview  {}/{}",
        selected + 1,
        plan.commits.len()
    ))
    .block(Block::default().borders(Borders::ALL).title("Paladin"));
    frame.render_widget(header, areas[0]);

    let body = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(areas[1]);

    let items = plan
        .commits
        .iter()
        .enumerate()
        .map(|(index, commit)| ListItem::new(format!("{}. {}", index + 1, commit.commit_message())))
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Commits"))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, body[0], &mut state);

    let commit = &plan.commits[selected];
    let mut details = vec![
        Line::from(vec![Span::styled(
            commit.commit_message(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(format!("Risk: {}", commit.risk)),
        Line::from(""),
        Line::from("Body:"),
    ];

    if commit.body.is_empty() {
        details.push(Line::from("- no body"));
    } else {
        details.extend(
            commit
                .body
                .iter()
                .map(|line| Line::from(format!("- {}", line.trim().trim_start_matches("- ")))),
        );
    }

    details.push(Line::from(""));
    details.push(Line::from("Files:"));
    details.extend(
        commit
            .files
            .iter()
            .map(|path| Line::from(format!("- {}", path))),
    );

    let details = Paragraph::new(details)
        .block(Block::default().borders(Borders::ALL).title("Preview"))
        .wrap(Wrap { trim: false });
    frame.render_widget(Clear, body[1]);
    frame.render_widget(details, body[1]);

    let footer_text = if diff_truncated {
        "Arrows: switch commit  Enter/Esc/q: exit preview  Note: diff was truncated"
    } else {
        "Arrows: switch commit  Enter/Esc/q: exit preview"
    };
    let footer =
        Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, areas[2]);
}

#[cfg(test)]
mod tests {
    use super::{parse_commit_plan, parse_commit_suggestion};

    #[test]
    fn parses_standard_commit_suggestion() {
        let raw = r#"{"type":"fix","scope":null,"subject":"handle parser retry prompt","body":["tighten retry instructions"],"risk":"low"}"#;
        let suggestion = parse_commit_suggestion(raw, &[String::from("src/commit/mod.rs")]).unwrap();

        assert_eq!(suggestion.commit_type, "fix");
        assert_eq!(suggestion.subject, "handle parser retry prompt");
        assert_eq!(suggestion.risk, "low");
        assert_eq!(suggestion.files, vec![String::from("src/commit/mod.rs")]);
    }

    #[test]
    fn parses_nested_commit_suggestion() {
        let raw = r#"{"result":{"type":"docs","scope":"readme","subject":"clarify commit json output","body":["show a valid example"],"risk":"low"}}"#;
        let suggestion = parse_commit_suggestion(raw, &[String::from("README.md")]).unwrap();

        assert_eq!(suggestion.commit_type, "docs");
        assert_eq!(suggestion.scope.as_deref(), Some("readme"));
        assert_eq!(suggestion.subject, "clarify commit json output");
    }

    #[test]
    fn parses_final_commit_plan_with_expected_files() {
        let raw = r#"{
          "strategy":"group related auth and docs changes separately",
          "commits":[
            {
              "type":"fix",
              "scope":"auth",
              "subject":"handle login retry flow",
              "body":["keep auth changes together"],
              "risk":"medium",
              "files":["src/auth.rs"]
            },
            {
              "type":"docs",
              "scope":"readme",
              "subject":"document login retry behavior",
              "body":[],
              "risk":"low",
              "files":["README.md"]
            }
          ]
        }"#;

        let plan = parse_commit_plan(raw, &[String::from("src/auth.rs"), String::from("README.md")])
            .unwrap();

        assert_eq!(plan.commits.len(), 2);
        assert_eq!(plan.commits[0].commit_type, "fix");
        assert_eq!(plan.commits[1].commit_type, "docs");
    }
}
