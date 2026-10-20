use std::{
    fs::File,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    MODEL_FILE, MODEL_NAME, download,
    http::Ollama,
    model_files::{self, SavedModel},
};

const QUICK: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum DevicePreference {
    #[default]
    Automatic,
    Cpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallPhase {
    Downloading,
    Importing,
    Warming,
}

#[derive(Clone, Debug)]
pub struct InstallProgress {
    pub phase: InstallPhase,
    pub completed: u64,
    pub total: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelStatus {
    /// Hearthspace registered this model for session readiness.
    /// Installation alone does not imply that the user enabled local AI.
    pub registered: bool,
    pub installed: bool,
    pub downloaded: bool,
    pub warm: bool,
    pub device: DevicePreference,
    pub runtime_error: Option<String>,
}

#[derive(Clone)]
pub struct ModelManager {
    ollama: Ollama,
    directory: PathBuf,
}

impl ModelManager {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            ollama: Ollama::from_env()?,
            directory: model_files::directory()?,
        })
    }

    pub fn new(endpoint: &str, directory: PathBuf) -> Result<Self> {
        Ok(Self {
            ollama: Ollama::new(endpoint)?,
            directory,
        })
    }

    pub fn status(&self) -> Result<ModelStatus> {
        let saved = model_files::saved(&self.directory)?;
        let mut status = ModelStatus {
            registered: saved.registered,
            installed: saved.registered,
            downloaded: model_files::cache_present(&self.directory),
            device: saved.device,
            ..ModelStatus::default()
        };
        let result: Result<()> = (|| {
            let (code, value) = self.ollama.request(
                Method::POST,
                "/api/show",
                Some(&json!({"model": MODEL_NAME})),
                QUICK,
            )?;
            match code {
                StatusCode::NOT_FOUND => {
                    status.installed = false;
                    return Ok(());
                }
                code if code.is_success() => status.installed = true,
                _ => bail!("Ollama could not inspect D1: {value}"),
            }
            let running = self
                .ollama
                .json(Method::GET, "/api/ps", None::<&Value>, QUICK)?;
            status.warm = running["models"].as_array().is_some_and(|models| {
                models
                    .iter()
                    .any(|model| model["name"] == MODEL_NAME || model["model"] == MODEL_NAME)
            });
            Ok(())
        })();
        status.runtime_error = result.err().map(|error| format!("{error:#}"));
        Ok(status)
    }

    /// Explicit user action only: no startup or gather call downloads a model.
    pub fn install(
        &self,
        device: DevicePreference,
        progress: impl Fn(InstallProgress) + Send + Sync + 'static,
    ) -> Result<()> {
        let _lock = model_files::lock(&self.directory)?;
        self.check_decision_api()?;
        let progress: Arc<dyn Fn(InstallProgress) + Send + Sync> = Arc::new(progress);
        progress(InstallProgress {
            phase: InstallPhase::Downloading,
            completed: 0,
            total: None,
        });
        let path = download::fetch(&self.directory, progress.clone())?;
        progress(InstallProgress {
            phase: InstallPhase::Importing,
            completed: 0,
            total: None,
        });
        let digest = download::digest(&path)?;
        self.ollama
            .upload(&format!("/api/blobs/{digest}"), File::open(path)?)?;
        self.create(json!({"files": {MODEL_FILE: digest}}), device)?;
        model_files::save(
            &self.directory,
            &SavedModel {
                device,
                registered: true,
            },
        )?;
        progress(InstallProgress {
            phase: InstallPhase::Warming,
            completed: 0,
            total: None,
        });
        self.warm_locked()
    }

    pub fn set_device(&self, device: DevicePreference) -> Result<()> {
        let _lock = model_files::lock(&self.directory)?;
        let status = self.status()?;
        if status.installed
            && let Some(error) = status.runtime_error
        {
            bail!("{error}");
        }
        if status.installed {
            self.unload()?;
            self.create(json!({"from": MODEL_NAME}), device)?;
        }
        model_files::save(
            &self.directory,
            &SavedModel {
                device,
                registered: status.installed,
            },
        )?;
        if status.installed {
            self.warm_locked()?;
        }
        Ok(())
    }

    /// Discard only this application's cache and managed Ollama model.
    /// If Ollama is offline the cache is still removed; the error tells the user
    /// to retry removing the runtime copy once Ollama is available.
    pub fn remove(&self) -> Result<()> {
        let _lock = model_files::lock(&self.directory)?;
        model_files::remove_cache(&self.directory)?;
        self.unload().context(
            "Download cache deleted; start Ollama and retry to unload and remove its D1 copy",
        )?;
        let (code, value) = self
            .ollama
            .request(
                Method::DELETE,
                "/api/delete",
                Some(&json!({"model": MODEL_NAME})),
                QUICK,
            )
            .context("Download cache deleted; start Ollama and retry to remove its D1 copy")?;
        if !code.is_success() && code != StatusCode::NOT_FOUND {
            bail!("Download cache deleted, but Ollama could not delete D1: {value}");
        }
        let mut saved = model_files::saved(&self.directory)?;
        saved.registered = false;
        model_files::save(&self.directory, &saved)
    }

    pub fn warm_if_installed(&self) -> Result<bool> {
        self.warm_installed(300)
    }

    /// Renew every two minutes during the session. A five-minute lease also
    /// releases the model if the session is killed before clean shutdown.
    pub fn warm_for_session(&self) -> Result<bool> {
        self.warm_installed(300)
    }

    fn warm_installed(&self, keep_alive: i64) -> Result<bool> {
        // Optional means no network or cache creation on ordinary sessions.
        if !model_files::saved(&self.directory)?.registered {
            return Ok(false);
        }
        let _lock = model_files::lock(&self.directory)?;
        let status = self.status()?;
        if !status.installed {
            return Ok(false);
        }
        if let Some(error) = status.runtime_error {
            bail!("{error}");
        }
        self.warm_with_lease(keep_alive)?;
        Ok(true)
    }

    pub fn warm(&self) -> Result<()> {
        let _lock = model_files::lock(&self.directory)?;
        self.warm_locked()
    }

    /// Explicit opt-in for a model already present in Ollama; never downloads.
    /// Keep the registration after a load error so readiness can be retried.
    pub fn enable_installed(&self) -> Result<()> {
        let _lock = model_files::lock(&self.directory)?;
        let status = self.status()?;
        if let Some(error) = status.runtime_error {
            bail!("{error}");
        }
        if !status.installed {
            bail!("The local model is no longer installed. Turn on AI to download it again.");
        }
        model_files::save(
            &self.directory,
            &SavedModel {
                device: status.device,
                registered: true,
            },
        )?;
        self.warm_locked()
    }

    fn warm_locked(&self) -> Result<()> {
        self.warm_with_lease(300)
    }

    fn warm_with_lease(&self, keep_alive: i64) -> Result<()> {
        self.ollama
            .json(
                Method::POST,
                "/v1/systemone",
                Some(&json!({
                    "model": MODEL_NAME, "keep_alive": keep_alive, "state": "Hearthspace desktop",
                    "questions": {"ready": {"type": "noul", "instructions": "Is this a desktop?"}}
                })),
                Duration::from_secs(120),
            )
            .context(
                "Cannot load D1; install the model and use Ollama with /v1/systemone support",
            )?;
        Ok(())
    }

    /// Ollama handles an empty unload before checking text generation support.
    pub fn unload(&self) -> Result<()> {
        let deadline = Instant::now() + QUICK;
        let (status, value) = self.ollama.request(
            Method::POST,
            "/api/generate",
            Some(&json!({"model": MODEL_NAME, "keep_alive": 0})),
            QUICK,
        )?;
        if !status.is_success() && status != StatusCode::NOT_FOUND {
            bail!("Could not unload D1: {value}");
        }
        if status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        // Ollama acknowledges expiry before the runner actually stops. Confirm
        // it stopped before deleting or replacing device parameters.
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("D1 is still in use; retry after gathering finishes");
            }
            let running = self
                .ollama
                .json(Method::GET, "/api/ps", None::<&Value>, remaining)?;
            let models = running["models"]
                .as_array()
                .context("Ollama returned no running-model list")?;
            if !models
                .iter()
                .any(|model| model["name"] == MODEL_NAME || model["model"] == MODEL_NAME)
            {
                return Ok(());
            }
            std::thread::sleep(
                Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn create(&self, mut body: Value, device: DevicePreference) -> Result<()> {
        body["model"] = MODEL_NAME.into();
        body["stream"] = false.into();
        body["parameters"] = json!({"num_ctx": 32768,
            "num_gpu": if device == DevicePreference::Cpu { 0 } else { -1 }});
        self.ollama.json(
            Method::POST,
            "/api/create",
            Some(&body),
            Duration::from_secs(120),
        )?;
        Ok(())
    }

    fn check_decision_api(&self) -> Result<()> {
        let (status, _) =
            self.ollama
                .request(Method::POST, "/v1/systemone", Some(&json!({})), QUICK)?;
        if status != StatusCode::BAD_REQUEST {
            bail!(
                "This Ollama version lacks the D1 decision API; update Ollama for /v1/systemone support"
            );
        }
        Ok(())
    }
}
