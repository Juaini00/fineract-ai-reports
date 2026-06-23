use std::time::Duration;

use anyhow::{Context, Result, bail};
use app_core::config::VoyageAiConfig;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct VoyageEmbeddingClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    dimensions: i32,
}

impl VoyageEmbeddingClient {
    pub fn new(config: &VoyageAiConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .context("build Voyage AI HTTP client")?;

        Ok(Self {
            http,
            api_key: config.api_key.clone(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            model: config.embedding_model.clone(),
            dimensions: config.embedding_dimensions,
        })
    }

    pub async fn embed_documents(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed(inputs, "document").await
    }

    pub async fn embed_query(&self, input: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(&[input.to_string()], "query").await?;
        embeddings
            .into_iter()
            .next()
            .context("Voyage AI returned no query embedding")
    }

    async fn embed(&self, inputs: &[String], input_type: &str) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        if self.api_key.trim().is_empty() {
            bail!("VOYAGEAI_API_KEY is required when catalog embedding sync is enabled");
        }

        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "input": inputs,
                "model": self.model,
                "input_type": input_type,
                "output_dimension": self.dimensions,
            }))
            .send()
            .await
            .context("request Voyage AI embeddings")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("Voyage AI embedding request failed with status {status}: {body}");
        }

        let payload: VoyageEmbeddingResponse = response
            .json()
            .await
            .context("parse Voyage AI embedding response")?;
        let embeddings = payload
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect::<Vec<_>>();

        if embeddings.len() != inputs.len() {
            bail!(
                "Voyage AI returned {} embeddings for {} inputs",
                embeddings.len(),
                inputs.len()
            );
        }

        Ok(embeddings)
    }
}

#[derive(Debug, Deserialize)]
struct VoyageEmbeddingResponse {
    data: Vec<VoyageEmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct VoyageEmbeddingItem {
    embedding: Vec<f32>,
}
