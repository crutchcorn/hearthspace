use super::*;
#[path = "app_registration_identity_tests.rs"]
mod identity;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn catalog(directory: &Path, count: usize) -> AppCatalog {
    let applications = directory.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    for index in 0..count {
        std::fs::write(
            applications.join(format!("{index:04}{}.desktop", "x".repeat(110))),
            format!("[Desktop Entry]\nType=Application\nName=App {index:04}\nExec=true\n"),
        )
        .unwrap();
    }
    AppCatalog::load_from_data_dirs(vec![directory.to_owned()])
}

fn ids(catalog: &AppCatalog) -> Vec<String> {
    let mut ids: Vec<_> = catalog.apps().iter().map(|app| app.id.clone()).collect();
    ids.sort_unstable();
    ids
}

fn reply(listener: &UnixListener, expected: &str, response: &[u8]) {
    let (stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), expected);
    reader.get_mut().write_all(response).unwrap();
}

#[test]
fn complete_raw_catalog_exceeds_tile_cap_but_each_registration_is_bounded() {
    let ids: Vec<_> = (0..5000)
        .map(|index| format!("org.example.App{index:04}.desktop"))
        .collect();
    let batches = commands(&ids).unwrap();
    assert!(batches.len() > 1);
    let mut received = Vec::new();
    for command in batches {
        assert!(command.len() < hearthspace_ipc::transport::MAX_REQUEST_BYTES);
        let value: serde_json::Value =
            serde_json::from_str(command.strip_prefix("desktop-app-register ").unwrap()).unwrap();
        assert_eq!(value["offset"], received.len());
        received.extend(
            value["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned()),
        );
    }
    assert_eq!(received, ids);
    assert!(commands(&["x".repeat(hearthspace_ipc::transport::MAX_REQUEST_BYTES)]).is_err());
}

#[test]
fn registration_refreshes_catalog_membership_without_tile_changes_and_clears_removals() {
    let directory = tempfile::tempdir().unwrap();
    let first = catalog(directory.path(), 2);
    let first_ids = ids(&first);
    std::fs::write(
        &first.apps()[0].path,
        "[Desktop Entry]\nType=Application\nName=Renamed\nExec=true\n",
    )
    .unwrap();
    let renamed = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    let added = catalog(directory.path(), 3);
    std::fs::remove_file(&first.apps()[0].path).unwrap();
    let removed = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let expected: Vec<_> = [&first, &added, &removed, &AppCatalog::default()]
        .into_iter()
        .flat_map(|catalog| commands(&ids(catalog)).unwrap())
        .collect();
    let server = std::thread::spawn(move || {
        for command in expected {
            reply(&listener, &command, b"ok\n");
        }
    });
    let mut registration = AppRegistration::default();
    registration.sync(&socket, &first).unwrap();
    assert_eq!(registration.ids.as_ref(), Some(&first_ids));
    // Unchanged membership (including a name/order change) emits no reset.
    registration.sync(&socket, &first).unwrap();
    registration.sync(&socket, &renamed).unwrap();
    registration.sync(&socket, &added).unwrap();
    registration.sync(&socket, &removed).unwrap();
    assert_eq!(registration.ids.as_ref(), Some(&ids(&removed)));
    registration.sync(&socket, &AppCatalog::default()).unwrap();
    registration.sync(&socket, &AppCatalog::default()).unwrap();
    assert_eq!(registration.ids, Some(vec![]));
    server.join().unwrap();
}

#[test]
fn app_menu_waits_for_all_batches_and_partial_failure_retries_from_offset_zero() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = catalog(directory.path(), 200);
    let batches = commands(&ids(&catalog)).unwrap();
    assert!(batches.len() >= 2);
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let app_id = catalog.apps().last().unwrap().id.clone();
    let expected_app = app_id.clone();
    let server = std::thread::spawn(move || {
        reply(&listener, &batches[0], b"ok\n");
        reply(&listener, &batches[1], b"err fixture rejected batch\n");
        // The next attempt must restart the complete registration before opening.
        for command in batches {
            reply(&listener, &command, b"ok\n");
        }
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let value: serde_json::Value = serde_json::from_str(
            line.trim_end()
                .strip_prefix("desktop-app-context ")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["app_id"], expected_app);
        assert_eq!(value["popup_epoch"], 17);
        assert_eq!(value["intent"], "ordinary");
        reader.get_mut().write_all(b"ok\n").unwrap();
    });
    let request = || {
        let mut request = app_context::new_request(
            app_context::Target {
                app_id: app_id.clone(),
                output_name: Some("eDP-1".into()),
                source_app_id: "dev.hearthspace.launcher".into(),
            },
            xilem::masonry::kurbo::Rect::new(10.0, 10.0, 90.0, 50.0),
            Arc::new(AtomicBool::new(true)),
        );
        request.popup_epoch = 17;
        request
    };
    let mut registration = AppRegistration::default();
    assert!(
        registration
            .open(&socket, &catalog, &request())
            .unwrap_err()
            .contains("fixture rejected batch")
    );
    assert!(registration.ids.is_none());
    registration.open(&socket, &catalog, &request()).unwrap();
    assert_eq!(registration.ids.as_ref(), Some(&ids(&catalog)));
    server.join().unwrap();
}

#[test]
fn source_cancellation_during_registration_cannot_open_a_delayed_menu() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = catalog(directory.path(), 200);
    let batches = commands(&ids(&catalog)).unwrap();
    assert!(batches.len() >= 2);
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let alive = Arc::new(AtomicBool::new(true));
    let cancel = alive.clone();
    let server = std::thread::spawn(move || {
        for command in batches {
            reply(&listener, &command, b"ok\n");
            cancel.store(false, Ordering::Release);
        }
    });
    let request = app_context::new_request(
        app_context::Target {
            app_id: catalog.apps()[0].id.clone(),
            output_name: None,
            source_app_id: "dev.hearthspace.launcher".into(),
        },
        xilem::masonry::kurbo::Rect::new(10.0, 10.0, 90.0, 50.0),
        alive,
    );
    let mut registration = AppRegistration::default();
    let error = registration.open(&socket, &catalog, &request).unwrap_err();
    assert!(error.contains("expired"), "{error}");
    assert_eq!(registration.ids.as_ref(), Some(&ids(&catalog)));
    server.join().unwrap();
}
