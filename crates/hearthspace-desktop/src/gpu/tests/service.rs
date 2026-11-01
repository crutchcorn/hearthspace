//! A private broker substitutes only switcheroo-control; the resolver and child
//! command environment are real. No host system service is queried by these tests.
use super::*;
use crate::file_launch::test_support::{Bus, Fixture, assert_success, child_root};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const CHILD: &str = "gpu::tests::service::resolve_in_isolated_environment";

struct Switcheroo {
    reads: Arc<AtomicUsize>,
    delay: Duration,
    timer: tokio::runtime::Handle,
}

#[zbus::interface(name = "net.hadess.SwitcherooControl")]
impl Switcheroo {
    #[zbus(property, name = "GPUs")]
    async fn gpus(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        // zbus owns an async-io executor; its callbacks are outside Tokio.
        let delay = self.delay;
        self.timer
            .spawn(async move { tokio::time::sleep(delay).await })
            .await
            .unwrap();
        vec![
            device(true, Some(false), &[]),
            device(
                false,
                Some(true),
                &[
                    "DRI_PRIME",
                    "pci-0000_01_00_0",
                    "__NV_PRIME_RENDER_OFFLOAD",
                    "1",
                    "__GLX_VENDOR_LIBRARY_NAME",
                    "nvidia",
                ],
            ),
        ]
    }
}

async fn run(fixture: &Fixture, bus: &Bus, mode: &str) {
    let mut command = fixture.child(CHILD);
    command
        .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
        .env("GPU_TEST_MODE", mode);
    tokio::task::spawn_blocking(move || assert_success(&mut command))
        .await
        .unwrap();
}

#[tokio::test]
async fn private_service_variables_reach_only_the_requested_child_and_unchecked_does_not_query() {
    let fixture = Fixture::new();
    let bus = Bus::start(fixture.path());
    let reads = Arc::new(AtomicUsize::new(0));
    let _service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .serve_at(
            PATH,
            Switcheroo {
                reads: reads.clone(),
                delay: Duration::ZERO,
                timer: tokio::runtime::Handle::current(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    run(&fixture, &bus, "selected").await;
    assert!(reads.load(Ordering::SeqCst) > 0);
    reads.store(0, Ordering::SeqCst);
    run(&fixture, &bus, "unchecked").await;
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn missing_service_recovers_on_a_later_launch_and_delayed_service_is_bounded() {
    let fixture = Fixture::new();
    let bus = Bus::start(fixture.path());
    run(&fixture, &bus, "unavailable").await;
    let service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .serve_at(
            PATH,
            Switcheroo {
                reads: Default::default(),
                delay: Duration::ZERO,
                timer: tokio::runtime::Handle::current(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    run(&fixture, &bus, "selected").await;
    service.release_name(SERVICE).await.unwrap();
    drop(service);
    let _delayed = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .serve_at(
            PATH,
            Switcheroo {
                reads: Default::default(),
                delay: Duration::from_secs(2),
                timer: tokio::runtime::Handle::current(),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    run(&fixture, &bus, "unavailable").await;
}

#[test]
fn resolve_in_isolated_environment() {
    let Some(root) = child_root() else { return };
    let mode = std::env::var("GPU_TEST_MODE").unwrap();
    let started = Instant::now();
    let environment = launch_environment(mode != "unchecked");
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "lookup exceeded its bound"
    );
    let selected = mode == "selected";
    assert_eq!(!environment.is_empty(), selected);
    assert!(
        std::env::var_os("DRI_PRIME").is_none(),
        "lookup mutated the parent"
    );
    let envs = environment
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let command = crate::app_catalog::launch_command(
        &["/bin/sh".into(), "-c".into(),
            "printf '%s|%s|%s|%s' \"$DRI_PRIME\" \"$__NV_PRIME_RENDER_OFFLOAD\" \"$__GLX_VENDOR_LIBRARY_NAME\" \"$WAYLAND_DISPLAY\"".into()],
        "wayland-gpu-child", &envs, Some(&root),
    ).unwrap();
    let output = child_output(command);
    assert_eq!(
        output,
        if selected {
            "pci-0000_01_00_0|1|nvidia|wayland-gpu-child"
        } else {
            "|||wayland-gpu-child"
        }
    );
    let ordinary = crate::app_catalog::launch_command(
        &[
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s' \"$DRI_PRIME\"".into(),
        ],
        "wayland-ordinary",
        &[],
        None,
    )
    .unwrap();
    assert_eq!(child_output(ordinary), "");
}

fn child_output(mut command: std::process::Command) -> String {
    let output = command.output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
