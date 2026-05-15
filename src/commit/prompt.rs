use crate::git::FileDiff;

pub fn build_batch_commit_prompt(batch: &[FileDiff]) -> String {
    let file_list = batch
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let file_diffs = batch
        .iter()
        .map(|file| format!("FILE: {}\n{}\n", file.path, file.diff.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        r#"
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
