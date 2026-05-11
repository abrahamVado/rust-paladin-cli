use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSuggestion {
    #[serde(rename = "type")]
    pub commit_type: String,
    pub scope: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: Vec<String>,
    pub risk_level: String,
    #[serde(default)]
    pub files: Vec<String>,
    pub should_commit: bool,
}

impl CommitSuggestion {
    pub fn validate(&self) -> Result<()> {
        validate_commit_type(&self.commit_type)?;
        validate_risk_level(&self.risk_level)?;

        if self.title.trim().is_empty() {
            return Err(anyhow!("commit title is empty"));
        }

        if self.title.trim().ends_with('.') {
            return Err(anyhow!("commit title must not end with a period"));
        }

        if self.title.chars().count() > 90 {
            return Err(anyhow!("commit title is too long"));
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
        let title = self.title.trim();

        match self.scope.as_ref().map(|value| value.trim()).filter(|value| !value.is_empty()) {
            Some(scope) => format!("{}({}): {}", self.commit_type.trim(), scope, title),
            None => format!("{}: {}", self.commit_type.trim(), title),
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

fn validate_risk_level(value: &str) -> Result<()> {
    let allowed = ["low", "medium", "high"];

    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(anyhow!("invalid risk level: {}", value))
    }
}
