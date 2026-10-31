use super::*;
use zbus::zvariant::{OwnedValue, Value};

struct Switcheroo;

#[zbus::interface(name = "net.hadess.SwitcherooControl")]
impl Switcheroo {
    #[zbus(property, name = "GPUs")]
    fn gpus(&self) -> Vec<HashMap<String, OwnedValue>> {
        vec![HashMap::from([
            ("Default".into(), OwnedValue::from(false)),
            ("Discrete".into(), OwnedValue::from(true)),
            (
                "Environment".into(),
                OwnedValue::try_from(Value::from(vec![
                    "DRI_PRIME".to_owned(),
                    "fixture-discrete".to_owned(),
                    "__NV_PRIME_RENDER_OFFLOAD".to_owned(),
                    "1".to_owned(),
                ]))
                .unwrap(),
            ),
        ])]
    }
}

#[tokio::test]
async fn gpu_preference_reaches_the_scoped_child_without_cold_bus_activation() {
    let fixture = fixture();
    let desktop = fixture
        .path()
        .join(format!("share/applications/{APP}.desktop"));
    let contents = fs::read_to_string(&desktop).unwrap();
    fs::write(
        &desktop,
        format!("{contents}PrefersNonDefaultGPU=true\nNoDisplay=true\n"),
    )
    .unwrap();
    let bus = Bus::start_with_services(fixture.path());
    let _service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("net.hadess.SwitcherooControl")
        .unwrap()
        .serve_at("/net/hadess/SwitcherooControl", Switcheroo)
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut command = fixture.child(LAUNCH);
    command
        .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
        .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
        .env("HEARTHSPACE_TEST_LAUNCH_MODE", "scoped-gpu");
    tokio::task::spawn_blocking(move || assert_success(&mut command))
        .await
        .unwrap();
    wait_for(&fixture.path().join("ready"));
    assert!(!fixture.path().join("broker-started").exists());
    assert_eq!(
        fs::read_to_string(fixture.path().join("gpu-environment")).unwrap(),
        "fixture-discrete\n1\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("environment")).unwrap(),
        format!(
            "{}\n{}\nwayland-scoped\n:91\nowned\n",
            bus.address,
            fixture.path().join("private-profile").display(),
        )
    );
    let arguments = fs::read(fixture.path().join("arguments")).unwrap();
    assert_eq!(
        arguments
            .strip_suffix(&[0])
            .unwrap()
            .split(|byte| *byte == 0)
            .collect::<Vec<_>>(),
        [
            filename(fixture.path()).as_os_str().as_encoded_bytes(),
            desktop.as_os_str().as_encoded_bytes(),
            b"Cold Player".as_slice(),
        ]
    );
}
