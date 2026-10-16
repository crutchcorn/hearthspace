use std::{io::Read, net::IpAddr, time::Duration};

use anyhow::{Context, Result, bail};
use reqwest::{Method, StatusCode, Url, blocking::Client};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone)]
pub(crate) struct Ollama {
    client: Client,
    base: Url,
}

impl Ollama {
    pub fn from_env() -> Result<Self> {
        Self::new(
            &std::env::var("HEARTHSPACE_OLLAMA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".into()),
        )
    }

    pub fn new(endpoint: &str) -> Result<Self> {
        let base = Url::parse(endpoint).context("Invalid local Ollama URL")?;
        let loopback = match base.host_str() {
            Some("localhost") => true,
            Some(host) => host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback()),
            None => false,
        };
        if !loopback
            || base.scheme() != "http"
            || !base.username().is_empty()
            || base.password().is_some()
            || base.path() != "/"
            || base.query().is_some()
            || base.fragment().is_some()
        {
            bail!("Ollama must use an HTTP loopback URL without credentials or a path");
        }
        Ok(Self {
            client: Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(60))
                .build()?,
            base,
        })
    }

    pub fn request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&T>,
        timeout: Duration,
    ) -> Result<(StatusCode, Value)> {
        let mut request = self
            .client
            .request(method, self.base.join(path)?)
            .timeout(timeout);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .context("Cannot reach local Ollama; start the Ollama service")?;
        let status = response.status();
        let mut bytes = Vec::new();
        response.take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            bail!("Ollama response exceeded the 1 MiB limit");
        }
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) if !status.is_success() => serde_json::json!({
                    "error": String::from_utf8_lossy(&bytes).chars().take(512).collect::<String>()
                }),
                Err(error) => {
                    return Err(error).context("Ollama returned an invalid JSON response");
                }
            }
        };
        Ok((status, value))
    }

    pub fn json<T: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&T>,
        timeout: Duration,
    ) -> Result<Value> {
        let (status, value) = self.request(method, path, body, timeout)?;
        if !status.is_success() || value.get("error").is_some() {
            let detail = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("request failed");
            bail!("Ollama {path}: {status}: {detail}");
        }
        Ok(value)
    }

    pub fn upload(&self, path: &str, file: std::fs::File) -> Result<()> {
        let length = file.metadata()?.len();
        let response = self
            .client
            .post(self.base.join(path)?)
            .timeout(Duration::from_secs(600))
            .header(reqwest::header::CONTENT_LENGTH, length)
            .body(file)
            .send()
            .context("Could not import D1 into local Ollama")?;
        response.error_for_status()?;
        Ok(())
    }
}
