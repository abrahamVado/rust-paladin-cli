# Paladin

A local Rust CLI that uses a local Ollama model, for example `gemma4:e4b`, to generate a conventional commit plan from your current diff.

Paladin is designed to be safe:

- The model does not run shell commands.
- Rust runs `git` commands directly.
- `commit` previews the suggested commit plan in a TUI by default.
- `commit` can split large or mixed changes into multiple themed commits.
- `commit` sends smaller file batches to the model instead of one large diff blob.
- `commit` does a final whole-plan review so the model can check all parts together.
- `commit` retries when the model returns invalid JSON.
- `commit` asks for confirmation before creating any commit.
- The model must return JSON, which Paladin validates before use.

## Requirements

- Rust
- Git
- Ollama running locally
- A local model, for example:

```bash
ollama pull gemma4:e4b
```

Test Ollama:

```bash
curl http://localhost:11434/api/tags
```

## Build

```bash
cargo build --release
```

Binary:

```bash
./target/release/paladin
```

## Usage

Create a commit plan after preview and confirmation:

```bash
paladin commit
```

This opens an interactive terminal preview when possible. If your terminal does not support it, Paladin now falls back to a plain-text preview automatically.

Use only staged changes:

```bash
paladin commit --staged
```

Use another Ollama model:

```bash
paladin commit --model qwen2.5-coder:7b
```

Use another Ollama URL:

```bash
paladin commit --ollama-url http://192.168.0.223:11434
```

Disable confirmation:

```bash
paladin commit --yes
```

Disable the interactive preview:

```bash
paladin commit --no-tui
```

Allow more commit groups for large changes:

```bash
paladin commit --max-commits 7
```

Use smaller model requests for weaker local models:

```bash
paladin commit --max-files-per-batch 2 --max-batch-chars 2500 --max-file-diff-chars 900
```

## What Paladin runs

Depending on flags:

```bash
git status --short
git branch --show-current
git diff --stat
git diff --name-only
git diff
git add -A
git commit -m "..."
```

With `--staged`, it uses:

```bash
git diff --staged --stat
git diff --staged --name-only
git diff --staged
```

## Example

```bash
paladin commit
```

Output:

```text
Suggested commit plan:
Split auth logic from CLI wiring so each commit stays reviewable.

Commit 1
feat(auth): wire login flow to database

Body:
- validate credentials using stored bcrypt hashes
- return an access token after successful authentication

Files:
- src/auth.rs
- src/db.rs

Commit 2
refactor(cli): simplify login command flow

Files:
- src/cli.rs
```

## Notes

For very large diffs, Paladin truncates the diff before sending it to the local model. It also limits how much of any single file diff is included in one request. This keeps smaller local models from getting overloaded.

Commit splitting is file-based. Paladin can group related files into separate commits, but it does not try to split multiple themes inside the same file into different commits.
