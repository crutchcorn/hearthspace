use std::path::Path;

use serde_json::{Value, json};

use crate::{DevicePreference, MODEL_FILE, MODEL_NAME, ModelManager, model_files, tests::mock};

#[test]
fn unload_waits_until_the_runner_has_actually_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests, server) = mock(vec![
        (200, json!({"done":true})),
        (200, json!({"models":[{"name":MODEL_NAME}]})),
        (200, json!({"models":[]})),
    ]);
    ModelManager::new(&endpoint, temp.path().into())
        .unwrap()
        .unload()
        .unwrap();
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests[1].0, "GET /api/ps HTTP/1.1");
    assert_eq!(requests[2].0, "GET /api/ps HTTP/1.1");
}

fn seed_cache(directory: &Path) {
    let repository = directory.join("hub/models--LiquidAI--d1-3B-GGUF");
    std::fs::create_dir_all(repository.join("refs")).unwrap();
    std::fs::create_dir_all(repository.join("snapshots/test")).unwrap();
    std::fs::write(repository.join("refs/main"), "test").unwrap();
    std::fs::write(
        repository.join("snapshots/test").join(MODEL_FILE),
        b"test gguf contents",
    )
    .unwrap();
}

#[test]
fn install_imports_exact_download_and_warms_native_decision_model() {
    let temp = tempfile::tempdir().unwrap();
    seed_cache(temp.path());
    let (endpoint, requests, server) = mock(vec![
        (400, json!({"error":"model is required"})),
        (201, Value::Null),
        (200, json!({"status":"success"})),
        (200, json!({"answers":{"ready":{"noul":0.9}}})),
    ]);
    let manager = ModelManager::new(&endpoint, temp.path().into()).unwrap();
    manager.install(DevicePreference::Cpu, |_| {}).unwrap();
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests[1].0.starts_with("POST /api/blobs/sha256:"));
    assert_eq!(requests[1].1, b"test gguf contents");
    let create: Value = serde_json::from_slice(&requests[2].1).unwrap();
    assert!(create["files"].get(MODEL_FILE).is_some());
    assert_eq!(create["parameters"], json!({"num_ctx":32768,"num_gpu":0}));
    let warm: Value = serde_json::from_slice(&requests[3].1).unwrap();
    assert_eq!(warm["keep_alive"], 300);
    assert!(model_files::saved(temp.path()).unwrap().registered);
}

#[test]
fn optional_startup_avoids_all_ollama_requests_and_cache_creation() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("absent");
    let manager = ModelManager::new("http://127.0.0.1:9", directory.clone()).unwrap();
    assert!(!manager.warm_for_session().unwrap());
    assert!(!directory.exists());
}

#[test]
fn inspecting_an_existing_model_does_not_claim_hearthspace_registered_it() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _, server) = mock(vec![(200, json!({})), (200, json!({"models":[]}))]);
    let status = ModelManager::new(&endpoint, temp.path().into())
        .unwrap()
        .status()
        .unwrap();
    server.join().unwrap();
    assert!(status.installed);
    assert!(!status.registered);
    assert!(!status.warm);
    assert!(!temp.path().join("model.json").exists());
}

#[test]
fn explicitly_enabling_existing_model_registers_and_warms_without_download() {
    let temp = tempfile::tempdir().unwrap();
    model_files::save(
        temp.path(),
        &model_files::SavedModel {
            device: DevicePreference::Cpu,
            registered: false,
        },
    )
    .unwrap();
    let (endpoint, requests, server) = mock(vec![
        (200, json!({})),
        (200, json!({"models":[]})),
        (200, json!({"answers":{"ready":{"noul":0.9}}})),
        (200, json!({})),
        (200, json!({"models":[{"name":MODEL_NAME}]})),
        (200, json!({"answers":{"ready":{"noul":0.9}}})),
    ]);
    let manager = ModelManager::new(&endpoint, temp.path().into()).unwrap();
    manager.enable_installed().unwrap();
    let saved = model_files::saved(temp.path()).unwrap();
    assert!(saved.registered);
    assert_eq!(saved.device, DevicePreference::Cpu);
    assert!(!temp.path().join("hub").exists());
    assert!(
        manager.warm_for_session().unwrap(),
        "explicit enable joins the existing session keep-alive lifecycle"
    );
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.0.as_str())
            .collect::<Vec<_>>(),
        [
            "POST /api/show HTTP/1.1",
            "GET /api/ps HTTP/1.1",
            "POST /v1/systemone HTTP/1.1",
            "POST /api/show HTTP/1.1",
            "GET /api/ps HTTP/1.1",
            "POST /v1/systemone HTTP/1.1",
        ]
    );
}

#[test]
fn explicit_enable_preserves_opt_in_after_load_error_but_never_adopts_missing_model() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _, server) = mock(vec![
        (404, json!({"error":"model not found"})),
        (200, json!({})),
        (200, json!({"models":[]})),
        (500, json!({"error":"not enough memory"})),
    ]);
    let manager = ModelManager::new(&endpoint, temp.path().into()).unwrap();
    assert!(manager.enable_installed().is_err());
    assert!(!model_files::saved(temp.path()).unwrap().registered);
    assert!(manager.enable_installed().is_err());
    assert!(model_files::saved(temp.path()).unwrap().registered);
    assert!(!temp.path().join("hub").exists());
    server.join().unwrap();
}

#[test]
fn offline_status_and_delete_preserve_pending_runtime_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    seed_cache(temp.path());
    model_files::save(
        temp.path(),
        &model_files::SavedModel {
            device: DevicePreference::Cpu,
            registered: true,
        },
    )
    .unwrap();
    let manager = ModelManager::new("http://127.0.0.1:9", temp.path().into()).unwrap();
    let status = manager.status().unwrap();
    assert!(
        status.registered
            && status.installed
            && status.downloaded
            && status.runtime_error.is_some()
    );
    assert!(manager.remove().is_err());
    assert!(!temp.path().join("hub").exists());
    assert!(model_files::saved(temp.path()).unwrap().registered);
}

#[test]
fn delete_ignores_unrelated_models_and_caches() {
    let temp = tempfile::tempdir().unwrap();
    seed_cache(temp.path());
    std::fs::write(temp.path().join("unrelated"), "keep").unwrap();
    let (endpoint, requests, server) = mock(vec![
        (200, Value::Null),
        (200, json!({"models":[]})),
        (200, Value::Null),
    ]);
    ModelManager::new(&endpoint, temp.path().into())
        .unwrap()
        .remove()
        .unwrap();
    server.join().unwrap();
    assert!(temp.path().join("unrelated").exists());
    assert!(!temp.path().join("hub").exists());
    assert_eq!(requests.lock().unwrap()[0].0, "POST /api/generate HTTP/1.1");
    assert_eq!(requests.lock().unwrap()[2].0, "DELETE /api/delete HTTP/1.1");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests.lock().unwrap()[2].1).unwrap(),
        json!({"model":MODEL_NAME})
    );
}

#[test]
fn partial_download_remains_visible_and_removable() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hub/blobs")).unwrap();
    std::fs::write(temp.path().join("hub/blobs/partial.incomplete"), b"partial").unwrap();
    let (endpoint, _, server) = mock(vec![
        (404, json!({"error":"model not found"})),
        (404, Value::Null),
        (404, Value::Null),
    ]);
    let manager = ModelManager::new(&endpoint, temp.path().into()).unwrap();
    let status = manager.status().unwrap();
    assert!(!status.registered);
    assert!(!status.installed && status.downloaded);
    manager.remove().unwrap();
    server.join().unwrap();
    assert!(!temp.path().join("hub").exists());
}

#[test]
fn changing_device_reloads_only_managed_model_with_explicit_parameters() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, requests, server) = mock(vec![
        (200, json!({})),
        (200, json!({"models":[]})),
        (200, json!({})),
        (200, json!({"models":[]})),
        (200, json!({"status":"success"})),
        (200, json!({"answers":{"ready":{"noul":0.9}}})),
    ]);
    let manager = ModelManager::new(&endpoint, temp.path().into()).unwrap();
    manager.set_device(DevicePreference::Cpu).unwrap();
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests[2].0, "POST /api/generate HTTP/1.1");
    let unload: Value = serde_json::from_slice(&requests[2].1).unwrap();
    assert_eq!(unload["keep_alive"], 0);
    let create: Value = serde_json::from_slice(&requests[4].1).unwrap();
    assert_eq!(create["model"], MODEL_NAME);
    assert_eq!(create["from"], MODEL_NAME);
    assert_eq!(create["parameters"]["num_gpu"], 0);
    assert_eq!(
        model_files::saved(temp.path()).unwrap().device,
        DevicePreference::Cpu
    );
}

#[test]
fn old_ollama_is_rejected_before_download() {
    let temp = tempfile::tempdir().unwrap();
    let (endpoint, _, server) = mock(vec![(404, Value::Null)]);
    let error = ModelManager::new(&endpoint, temp.path().into())
        .unwrap()
        .install(DevicePreference::Automatic, |_| {})
        .unwrap_err();
    server.join().unwrap();
    assert!(error.to_string().contains("update Ollama"));
    assert!(!temp.path().join("hub").exists());
}

#[test]
fn lifecycle_operations_are_exclusive_between_processes() {
    let temp = tempfile::tempdir().unwrap();
    let _first = model_files::lock(temp.path()).unwrap();
    assert!(model_files::lock(temp.path()).is_err());
}

#[test]
fn cpu_preference_can_be_saved_offline_before_installation() {
    let temp = tempfile::tempdir().unwrap();
    ModelManager::new("http://127.0.0.1:9", temp.path().into())
        .unwrap()
        .set_device(DevicePreference::Cpu)
        .unwrap();
    let saved = model_files::saved(temp.path()).unwrap();
    assert_eq!(saved.device, DevicePreference::Cpu);
    assert!(!saved.registered);
}
