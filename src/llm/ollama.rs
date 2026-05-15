use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    http: Client,
}

#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    format: &'a str,
    options: GenerateOptions,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    temperature: f32,
    num_ctx: u32,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Debug, Clone)]
pub enum OllamaGenerateError {
    ModelLoadFailure { model: String, details: String },
    Http { status: u16, body: String },
    Transport(String),
    Parse(String),
}

impl std::fmt::Display for OllamaGenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelLoadFailure { model, details } => {
                write!(
                    f,
                    "Ollama could not load model `{}`. This usually means the model is too large for available resources or Ollama hit an internal error.\n{}",
                    model, details
                )
            }
            Self::Http { status, body } => write!(f, "Ollama returned HTTP {}:\n{}", status, body),
            Self::Transport(message) => write!(f, "{}", message),
            Self::Parse(message) => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for OllamaGenerateError {}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            http: Client::new(),
        }
    }

    pub async fn generate_json(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);

        let request = GenerateRequest {
            model: &self.model,
            prompt,
            stream: false,
            format: "json",
            options: GenerateOptions {
                temperature: 0.1,
                num_ctx: 8192,
            },
        };

        let response = self
            .http
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(|error| {
                OllamaGenerateError::Transport(format!(
                    "failed to call Ollama at `{}`; is Ollama running?\n{}",
                    self.base_url, error
                ))
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|error| {
            OllamaGenerateError::Transport(format!("failed to read Ollama response\n{}", error))
        })?;

        if !status.is_success() {
            if status.as_u16() == 500 && text.contains("model failed to load") {
                return Err(OllamaGenerateError::ModelLoadFailure {
                    model: self.model.clone(),
                    details: text,
                }
                .into());
            }

            return Err(OllamaGenerateError::Http {
                status: status.as_u16(),
                body: text,
            }
            .into());
        }

        let parsed: GenerateResponse = serde_json::from_str(&text).map_err(|error| {
            OllamaGenerateError::Parse(format!(
                "failed to parse Ollama generate response\n{}\nraw body:\n{}",
                error, text
            ))
        })?;

        Ok(parsed.response)
    }
}
