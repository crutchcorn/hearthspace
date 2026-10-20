use super::*;
use crate::{ModelManager, tests::mock};

#[test]
fn managed_gather_obeys_real_enable_and_remove_without_inference_while_off() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("ai");
    let (endpoint, requests, server) = mock(vec![
        // Explicitly adopt an existing model without a download.
        (200, json!({})),
        (200, json!({"models":[]})),
        (200, json!({"answers":{"ready":{"noul":0.9}}})),
        // Gather is permitted only after that opt-in.
        (200, json!({"answers":{"relevant":{"noul":0.95}}})),
        // The existing turn-off flow unloads and removes the model.
        (200, Value::Null),
        (200, json!({"models":[]})),
        (200, Value::Null),
    ]);
    // Supply the same persisted lifecycle directory as from_env without
    // mutating process-wide environment during parallel tests.
    let client = DecisionClient {
        ollama: Ollama::new(&endpoint).unwrap(),
        model_directory: Some(directory.clone()),
    };
    let manager = ModelManager::new(&endpoint, directory.clone()).unwrap();
    let group = ["Hearthspace editor".into()];
    let candidates = [WindowCandidate {
        id: 42,
        title: "Hearthspace documentation".into(),
    }];
    let cancel = AtomicBool::new(false);
    let disabled = client
        .gather(&group, &candidates, &cancel, |_, _| {
            panic!("disabled AI must not start inference progress")
        })
        .unwrap_err();
    assert!(disabled.to_string().contains("Turn on AI in Settings"));
    assert!(requests.lock().unwrap().is_empty());

    manager.enable_installed().unwrap();
    let mut progress = Vec::new();
    assert_eq!(
        client
            .gather(&group, &candidates, &cancel, |done, total| {
                progress.push((done, total));
            })
            .unwrap(),
        [42]
    );
    assert_eq!(progress, [(0, 1), (1, 1)]);
    manager.remove().unwrap();
    let disabled = client
        .gather(&group, &candidates, &cancel, |_, _| {
            panic!("turning AI off must stop future inference")
        })
        .unwrap_err();
    assert!(disabled.to_string().contains("Turn on AI in Settings"));
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert_eq!(requests[3].0, "POST /v1/systemone HTTP/1.1");
    assert_eq!(requests[6].0, "DELETE /api/delete HTTP/1.1");
    assert!(!directory.join("hub").exists(), "no model download");
}
