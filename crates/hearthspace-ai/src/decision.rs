use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::Method;
use serde_json::{Value, json};

use crate::{decision_request::relevance_request, http::Ollama, model_files};

const TITLE_LIMIT: usize = 512;
const GROUP_LIMIT: usize = 64;
const CANDIDATE_LIMIT: usize = 256;
const GATHER_BUDGET: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
pub struct WindowCandidate {
    pub id: u64,
    pub title: String,
}

#[derive(Clone)]
pub struct DecisionClient {
    pub(crate) ollama: Ollama,
    model_directory: Option<PathBuf>,
}

impl DecisionClient {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            ollama: Ollama::from_env()?,
            model_directory: Some(model_files::directory()?),
        })
    }

    /// Explicit loopback endpoint, useful for tests and nondefault local ports.
    pub fn new(endpoint: &str) -> Result<Self> {
        Ok(Self {
            ollama: Ollama::new(endpoint)?,
            model_directory: None,
        })
    }

    /// Compare every candidate against the same original group, sequentially.
    /// Cancellation is observed between requests; each request is bounded to 60s
    /// and the complete run to five minutes. No partial result is applied on error.
    pub fn gather(
        &self,
        group_titles: &[String],
        candidates: &[WindowCandidate],
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<u64>> {
        if group_titles.is_empty() || group_titles.iter().all(|title| title.trim().is_empty()) {
            bail!("Select a group with at least one titled window first");
        }
        if group_titles.len() > GROUP_LIMIT || candidates.len() > CANDIDATE_LIMIT {
            bail!(
                "Gather supports at most {GROUP_LIMIT} selected and {CANDIDATE_LIMIT} candidate windows"
            );
        }
        let titles: Vec<_> = group_titles
            .iter()
            .map(|title| bounded_title(title))
            .collect();
        let state = json!({"window_titles": titles});
        // Conservative byte budget leaves question/template room within D1's
        // 32K context and Ollama's 64KiB decision request cap.
        if serde_json::to_vec(&state)?.len() > 24_000 {
            bail!("Selected window titles exceed D1's input limit; select fewer windows");
        }
        // Settings and session workers share this process-independent guard.
        // Check opt-in while holding it so disabling AI cannot race inference.
        let _operation = self
            .model_directory
            .as_ref()
            .map(|directory| model_files::lock(directory))
            .transpose()?;
        if let Some(directory) = &self.model_directory
            && !model_files::saved(directory)?.registered
        {
            bail!("Turn on AI in Settings to gather similar windows.");
        }
        let deadline = Instant::now() + GATHER_BUDGET;
        let mut matched = Vec::new();
        progress(0, candidates.len());
        for (index, candidate) in candidates.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                bail!("Gather cancelled");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("Gather timed out; try a smaller group of windows");
            }
            if !candidate.title.trim().is_empty() {
                let title = bounded_title(&candidate.title);
                let request = relevance_request(&state, &title);
                let response = self.ollama.json(Method::POST, "/v1/systemone", Some(&request),
                    remaining.min(Duration::from_secs(60)))
                    .context("D1 decision failed; install D1 in Settings and use an Ollama version with /v1/systemone support")?;
                if relevance_probability(&response)? >= 0.5 {
                    matched.push(candidate.id);
                }
            }
            progress(index + 1, candidates.len());
        }
        if cancel.load(Ordering::Relaxed) {
            bail!("Gather cancelled");
        }
        Ok(matched)
    }
}

fn bounded_title(title: &str) -> String {
    title.chars().take(TITLE_LIMIT).collect()
}

fn relevance_probability(response: &Value) -> Result<f64> {
    let probability = response
        .pointer("/answers/relevant/noul")
        .and_then(Value::as_f64)
        .context("D1 returned no relevance probability")?;
    if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
        bail!("D1 returned an invalid relevance probability");
    }
    Ok(probability)
}

#[cfg(test)]
mod tests;
