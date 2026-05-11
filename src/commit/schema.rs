use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSuggestion {
    #[serde(rename = "type")]
    pub commit_type: String,
    pub scope: Option<String>,
    pub subject: String,
    #[serde(default)]
    pub body: Vec<String>,
}

impl CommitSuggestion {
    pub fn validate(&self) -> Result<()> {
        validate_commit_type(&self.commit_type)?;

        if self.subject.trim().is_empty() {
            return Err(anyhow!("commit subject is empty"));
        }

        if self.subject.trim().ends_with('.') {
            return Err(anyhow!("commit subject must not end with a period"));
        }

        if self.subject.chars().count() >= 72 {
            return Err(anyhow!("commit subject must be under 72 characters"));
        }

        if let Some(scope) = &self.scope {
            if scope.trim().is_empty() {
                return Err(anyhow!("commit scope is empty"));
            }

            if scope.contains(' ') {
                return Err(anyhow!("commit scope must not contain spaces"));
            }
        }

        Ok(())
    }

    pub fn commit_message(&self) -> String {
        let subject = self.subject.trim();

        match self.scope.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            Some(scope) => format!("{}({}): {}", self.commit_type.trim(), scope, subject),
            None => format!("{}: {}", self.commit_type.trim(), subject),
        }
    }
}

fn validate_commit_type(value: &str) -> Result<()> {
    let allowed = [
        "feat", "fix", "docs", "style", "refactor", "test", "chore", "build", "ci", "perf",
        "revert",
    ];

    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(anyhow!("invalid commit type: {}", value))
    }
}
