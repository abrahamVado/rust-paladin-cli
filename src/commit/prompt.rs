use crate::git::GitChangeSet;

pub fn build_commit_prompt(changes: &GitChangeSet) -> String {
    format!(
        r#"
You are a Git commit message generator.

Generate ONE conventional commit message for the provided Git changes.

Return ONLY valid JSON.
Do NOT return markdown.
Do NOT explain.
Do NOT reason.
Do NOT include a "thought" field.

Required JSON schema:

{{
  "type": "feat",
  "scope": null,
  "subject": "short imperative subject without period",
  "body": []
}}

Rules:
- "type" must be one of:
  feat, fix, docs, style, refactor, perf, test, build, ci, chore
- "scope" must be a string or null.
- "subject" must be under 72 characters.
- "subject" must not end with a period.
- "body" must be an array of strings.
- Use the summary and file list first.
- Use the diff preview only for extra context.
- Some generated or lockfile paths may appear in summaries but be omitted from the diff preview.
- If this looks like the first project commit, use type "feat" and subject like "add initial project scaffold".

Repository context:

Branch:
{}

Git status:
{}

Diff stat:
{}

Numstat:
{}

Changed files:
{}

Diff preview:
{}

Return ONLY the JSON object.
"#,
        changes.branch.output.trim(),
        changes.status_short.output.trim(),
        changes.diff_stat.output.trim(),
        changes.diff_numstat.output.trim(),
        changes.diff_name_only.output.trim(),
        changes.diff.output.trim()
    )
}
