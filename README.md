# Paladin

A local Rust CLI that uses a local Ollama model, for example `gemma4:e4b`, to generate conventional Git commits from your current diff.

Paladin is designed to be safe:

- The model does not run shell commands.
- Rust runs `git` commands directly.
- `commit` always previews the suggested commit.
- `commit` asks for confirmation before creating a commit.
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

Create a commit after preview and confirmation:

```bash
paladin commit
```

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
Suggested commit:
feat(auth): wire login flow to database

Body:
- validate credentials using stored bcrypt hashes
- return an access token after successful authentication

Risk: medium
```

## Notes

For very large diffs, Paladin truncates the diff before sending it to the local model. This keeps the small model from getting overloaded.
