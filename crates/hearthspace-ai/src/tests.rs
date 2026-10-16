use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use serde_json::{Value, json};

use crate::{DecisionClient, MODEL_NAME, WindowCandidate};

pub(crate) type Requests = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

pub(crate) fn mock(responses: Vec<(u16, Value)>) -> (String, Requests, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests: Requests = Arc::default();
    let received = requests.clone();
    let thread = thread::spawn(move || {
        for (code, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let header = String::from_utf8(header).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|length| length.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut request = vec![0; length];
            stream.read_exact(&mut request).unwrap();
            received
                .lock()
                .unwrap()
                .push((header.lines().next().unwrap().into(), request));
            let body = serde_json::to_vec(&body).unwrap();
            write!(stream, "HTTP/1.1 {code} response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (endpoint, requests, thread)
}

fn answer(probability: f64) -> (u16, Value) {
    (
        200,
        json!({"answers": {"relevant": {"type":"noul", "noul":probability}}}),
    )
}

fn candidate(id: u64, title: &str) -> WindowCandidate {
    WindowCandidate {
        id,
        title: title.into(),
    }
}

#[test]
fn gather_uses_fixed_group_and_independent_candidates() {
    let (endpoint, requests, server) = mock(vec![answer(0.96), answer(0.12)]);
    let client = DecisionClient::new(&endpoint).unwrap();
    let mut progress = Vec::new();
    let result = client
        .gather(
            &["Hearthspace editor".into()],
            &[
                candidate(7, "Hearthspace issues"),
                candidate(8, "Recipe book"),
            ],
            &AtomicBool::new(false),
            |done, total| progress.push((done, total)),
        )
        .unwrap();
    server.join().unwrap();
    assert_eq!(result, [7]);
    assert_eq!(progress, [(0, 2), (1, 2), (2, 2)]);
    let requests = requests.lock().unwrap();
    let bodies: Vec<Value> = requests
        .iter()
        .map(|(path, body)| {
            assert_eq!(path, "POST /v1/systemone HTTP/1.1");
            serde_json::from_slice(body).unwrap()
        })
        .collect();
    assert_eq!(
        bodies[0]["state"],
        json!({"window_titles":["Hearthspace editor"]})
    );
    assert_eq!(bodies[0]["state"], bodies[1]["state"]);
    assert_eq!(bodies[0]["model"], MODEL_NAME);
    assert_eq!(bodies[0]["keep_alive"], 300);
    assert_eq!(
        bodies[1]["questions"]["relevant"]["instructions"]["candidate_window_title"],
        "Recipe book"
    );
}

#[test]
fn malformed_decisions_are_errors_never_matches() {
    for body in [
        json!({}),
        json!({"answers":{"relevant":{"noul":1.5}}}),
        json!({"answers":{"relevant":{"noul":"true"}}}),
    ] {
        let (endpoint, _, server) = mock(vec![(200, body)]);
        let result = DecisionClient::new(&endpoint).unwrap().gather(
            &["Editor".into()],
            &[candidate(1, "Docs")],
            &AtomicBool::new(false),
            |_, _| {},
        );
        server.join().unwrap();
        assert!(result.is_err());
    }
}

#[test]
fn cancellation_discards_results_and_stops_before_next_request() {
    let (endpoint, requests, server) = mock(vec![answer(0.99)]);
    let cancel = AtomicBool::new(false);
    let result = DecisionClient::new(&endpoint).unwrap().gather(
        &["Editor".into()],
        &[candidate(1, "Docs"), candidate(2, "Issues")],
        &cancel,
        |done, _| {
            if done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    server.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[test]
fn rejects_remote_urls_and_bounds_inputs_before_contacting_server() {
    for endpoint in [
        "https://127.0.0.1",
        "http://example.com",
        "http://127.0.0.1/path",
        "http://user@127.0.0.1",
    ] {
        assert!(DecisionClient::new(endpoint).is_err());
    }
    let client = DecisionClient::new("http://127.0.0.1:9").unwrap();
    assert!(
        client
            .gather(&[], &[], &AtomicBool::new(false), |_, _| {})
            .is_err()
    );
    assert!(
        client
            .gather(
                &vec!["Title".into(); 65],
                &[],
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_err()
    );
    assert!(
        client
            .gather(
                &["Title".into()],
                &[candidate(1, "Docs")],
                &AtomicBool::new(true),
                |_, _| {}
            )
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
}

#[test]
fn binary_probability_threshold_is_inclusive_and_not_rounded() {
    let (endpoint, _, server) = mock(vec![answer(0.4999), answer(0.5), answer(0.5001)]);
    let result = DecisionClient::new(&endpoint)
        .unwrap()
        .gather(
            &["Hearthspace".into()],
            &[
                candidate(1, "Below"),
                candidate(2, "Equal"),
                candidate(3, "Above"),
            ],
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
    server.join().unwrap();
    assert_eq!(result, [2, 3]);
}

#[test]
fn context_budget_rejects_oversized_state_before_inference() {
    let client = DecisionClient::new("http://127.0.0.1:9").unwrap();
    assert!(
        client
            .gather(
                &vec!["a".repeat(512); 46],
                &[],
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_ok()
    );
    let error = client
        .gather(
            &vec!["a".repeat(512); 47],
            &[],
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap_err();
    assert!(error.to_string().contains("input limit"));
}
