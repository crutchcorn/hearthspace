//! Unsupported menu identities cannot poison the complete launch catalog.
use super::*;

#[test]
fn context_membership_matches_the_compositor_desktop_identity_boundary() {
    for id in [
        String::new(),
        "test.desktop".into(),
        "bad\n.desktop".into(),
        "bad\u{7f}.desktop".into(),
        "x".repeat(1020),
        "x".repeat(1021),
        "é".repeat(510),
        "é".repeat(511),
    ] {
        let supported = !id.is_empty()
            && hearthspace_common::desktop_item_id_from_app_id(
                &hearthspace_common::desktop_item_app_id(&format!("app:{id}")),
            )
            .is_some();
        assert_eq!(
            app_context::validate_app_id(&id).is_ok(),
            supported,
            "{id:?}"
        );
    }
}

#[test]
fn mixed_catalog_registers_valid_apps_and_rejects_only_unsupported_menu_targets() {
    let directory = tempfile::tempdir().unwrap();
    let valid = catalog(directory.path(), 1);
    let valid_id = valid.apps()[0].id.clone();
    let mut nested = directory.path().join("applications");
    for _ in 0..5 {
        nested.push("x".repeat(220));
    }
    std::fs::create_dir_all(&nested).unwrap();
    let path = nested.join("oversized.desktop");
    std::fs::write(
        &path,
        "[Desktop Entry]\nType=Application\nName=Oversized\nExec=true\n",
    )
    .unwrap();
    let mixed = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    assert_eq!(mixed.apps().len(), 2);
    let unsupported = mixed.apps().iter().find(|app| app.path == path).unwrap();
    assert!(unsupported.id.len() > 1020);
    // The complete catalog remains authoritative for ordinary launch resolution.
    assert_eq!(mixed.app_by_id(&unsupported.id).unwrap().exec, "true");
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let expected = commands(&ids(&valid)).unwrap();
    let expected_app = valid_id.clone();
    let server = std::thread::spawn(move || {
        for command in expected {
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
        assert_eq!(value["intent"], "accessibility");
        assert_eq!(value["popup_epoch"], 23);
        reader.get_mut().write_all(b"ok\n").unwrap();
        reply(&listener, &commands(&[]).unwrap()[0], b"ok\n");
    });
    let mut request = app_context::new_request(
        app_context::Target {
            app_id: valid_id,
            output_name: None,
            source_app_id: "dev.hearthspace.launcher".into(),
        },
        xilem::masonry::kurbo::Rect::new(10.0, 10.0, 90.0, 50.0),
        Arc::new(AtomicBool::new(true)),
    );
    request.intent = app_context::Intent::Accessibility;
    request.popup_epoch = 23;
    let mut registration = AppRegistration::default();
    registration.open(&socket, &mixed, &request).unwrap();
    assert_eq!(registration.ids.as_ref(), Some(&ids(&valid)));
    request.target.app_id = unsupported.id.clone();
    let error = registration.open(&socket, &mixed, &request).unwrap_err();
    assert!(
        error.contains("identifier") && error.contains("still launch"),
        "{error}"
    );
    // Unsupported membership changes do not reset valid menus. Removing the
    // last supported app still sends the empty replacement, even with bad IDs.
    registration.sync(&socket, &valid).unwrap();
    registration.sync(&socket, &mixed).unwrap();
    std::fs::remove_file(&valid.apps()[0].path).unwrap();
    let only_unsupported = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    registration.sync(&socket, &only_unsupported).unwrap();
    assert_eq!(registration.ids, Some(vec![]));
    assert!(only_unsupported.app_by_id(&unsupported.id).is_some());
    server.join().unwrap();
}
