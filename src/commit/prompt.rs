use crate::git::FileDiff;
use crate::commit::schema::CommitSuggestion;

pub fn build_batch_commit_prompt(batch: &[FileDiff], max_file_diff_chars: usize) -> String {
    let file_list = batch
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let file_diffs = batch
        .iter()
        .map(|file| {
            format!(
                "FILE: {}\n{}\n",
                file.path,
                truncate_chars(file.diff.trim(), max_file_diff_chars.max(200))
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"
Task: analyze the provided Git diff batch and write exactly one conventional commit suggestion for it.

You are a Git commit message generator.

Generate exactly ONE conventional commit for this small batch of related file changes.
Assume all listed files must belong to the same commit.

Return ONLY valid JSON.
Do NOT return markdown.
Do NOT explain.
Do NOT reason.
Do NOT include extra fields.
Do NOT return tool calls.

Required JSON schema:

{{
  "type": "feat",
  "scope": null,
  "subject": "short imperative subject without period",
  "body": [],
  "risk": "medium"
}}

Valid example:
{{"type":"fix","scope":"auth","subject":"handle missing login response body","body":["guard against nil response readers","keep error output concise"],"risk":"low"}}

Invalid examples:
- {{"error":"please tell me what to do"}}
- ```json ... ```
- Any reply with prose before or after the JSON object

Rules:
- "type" must be one of:
  feat, fix, docs, style, refactor, perf, test, build, ci, chore
- "scope" must be a string or null.
- "subject" must be under 72 characters.
- "subject" must not end with a period.
- "body" must be an array of strings.
- "risk" must be one of: low, medium, high.
- Describe the shared theme of the whole batch.
- Keep the subject specific but concise.

Files in this batch:
{file_list}

Per-file diffs:
{file_diffs}

Return ONLY the JSON object.
"#,
        file_list = file_list.trim(),
        file_diffs = file_diffs.trim(),
    )
}

pub fn build_plan_review_prompt(
    suggestions: &[CommitSuggestion],
    max_files_per_commit: usize,
) -> String {
    let allowed_files = suggestions
        .iter()
        .flat_map(|suggestion| suggestion.files.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n");

    let candidate_plan = suggestions
        .iter()
        .enumerate()
        .map(|(index, suggestion)| {
            format!(
                "Candidate {}:\n- message: {}\n- risk: {}\n- files:\n{}\n- body:\n{}",
                index + 1,
                suggestion.commit_message(),
                suggestion.risk,
                suggestion
                    .files
                    .iter()
                    .map(|path| format!("  - {}", path))
                    .collect::<Vec<_>>()
                    .join("\n"),
                if suggestion.body.is_empty() {
                    "  - none".to_string()
                } else {
                    suggestion
                        .body
                        .iter()
                        .map(|line| format!("  - {}", line.trim().trim_start_matches("- ")))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"
Task: review all candidate commit parts together and return one final commit plan.

You are refining a commit plan that was generated from several smaller diff batches.
Each candidate below covers a different part of the overall change set.

Return ONLY valid JSON.
Do NOT return markdown.
Do NOT explain.
Do NOT reason.
Do NOT include extra fields.
Do NOT return tool calls.

Required JSON schema:
{{
  "strategy": "short explanation of how the overall change set was grouped",
  "commits": [
    {{
      "type": "feat",
      "scope": null,
      "subject": "short imperative subject without period",
      "body": [],
      "risk": "medium",
      "files": ["src/example.rs"]
    }}
  ]
}}

Rules:
- Use every allowed file exactly once across the final plan.
- Do not invent files.
- Keep each commit focused and reviewable.
- Keep commit subjects under 72 characters and without a trailing period.
- "type" must be one of:
  feat, fix, docs, style, refactor, perf, test, build, ci, chore
- "risk" must be one of: low, medium, high.
- "files" must be an array of strings.
- Prefer between 1 and {max_files_per_commit} files per commit when reasonable.
- You may merge or regroup candidate parts, but every file must still appear exactly once.

Allowed files:
{allowed_files}

Candidate commit parts:
{candidate_plan}

Return ONLY the JSON object.
"#,
        allowed_files = allowed_files.trim(),
        candidate_plan = candidate_plan.trim(),
        max_files_per_commit = max_files_per_commit.max(1),
    )
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let truncated = value.chars().take(max_chars).collect::<String>();
    format!("{truncated}\n[per-file diff truncated by paladin]")
}
