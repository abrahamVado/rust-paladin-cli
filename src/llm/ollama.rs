use anyhow::{anyhow, Context, Result};
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
            .context("failed to call Ollama; is Ollama running?")?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("failed to read Ollama response")?;

        if !status.is_success() {
            return Err(anyhow!("Ollama returned HTTP {}:\n{}", status, text));
        }

        let parsed: GenerateResponse =
            serde_json::from_str(&text).context("failed to parse Ollama generate response")?;

        Ok(parsed.response)
    }
}
