//! Real D-Bus and Unix socket regression; no desktop, display, or host bus needed.
use std::{
    collections::HashMap,
    env, fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use futures_lite::StreamExt;
use serde_json::{Value, json};
use zbus::{Connection, Proxy};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

#[test]
fn private_bus_notification_lifecycle() {
    if env::var_os("HEARTHSPACE_NOTIFICATION_TEST_BUS").is_some() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(15), exercise_service())
                    .await
                    .unwrap();
            });
        return;
    }
    let runtime = tempfile::tempdir().unwrap();
    fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let child = Command::new("dbus-run-session")
        .arg("--")
        .arg(env::current_exe().unwrap())
        .args([
            "--exact",
            "private_bus_notification_lifecycle",
            "--nocapture",
        ])
        .env("HEARTHSPACE_NOTIFICATION_TEST_BUS", "1")
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .process_group(0)
        .spawn()
        .unwrap();
    let mut child = OwnedChild { child, group: true };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "private notification test failed: {status}"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "private notification test timed out"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

async fn exercise_service() {
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").unwrap());
    let socket = runtime.join("hearthspace-notifications.sock");
    let mut daemon = OwnedChild {
        child: Command::new(env!("CARGO_BIN_EXE_hearthspace-notifications"))
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
        group: false,
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !socket.exists() {
        assert!(
            daemon.child.try_wait().unwrap().is_none(),
            "notification daemon exited"
        );
        assert!(Instant::now() < deadline, "notification socket missing");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let connection = Connection::session().await.unwrap();
    let proxy = Proxy::new(&connection, NAME, PATH, NAME).await.unwrap();
    let other = Connection::session().await.unwrap();
    let other_proxy = Proxy::new(&other, NAME, PATH, NAME).await.unwrap();
    let capabilities: Vec<String> = proxy.call("GetCapabilities", &()).await.unwrap();
    assert_eq!(capabilities, ["actions", "body"]);
    let info: (String, String, String, String) =
        proxy.call("GetServerInformation", &()).await.unwrap();
    assert_eq!(info.0, "Hearthspace");
    assert_eq!(info.3, "1.3");
    let mut closed = proxy.receive_signal("NotificationClosed").await.unwrap();
    let mut invoked = proxy.receive_signal("ActionInvoked").await.unwrap();
    let mut foreign_signals = other_proxy.receive_signal("ActionInvoked").await.unwrap();

    let id = notify(&proxy, 0, "Hello", 0).await;
    let listed = request(&socket, json!({"command": "list"}));
    assert_eq!(listed["notifications"][0]["id"], id);
    assert_eq!(listed["notifications"][0]["body"], "Plain <b>text</b>");
    assert_eq!(listed["notifications"][0]["active"], true);
    let foreign_close: zbus::Result<()> = other_proxy.call("CloseNotification", &(id,)).await;
    assert!(foreign_close.is_err());
    assert_eq!(notify(&proxy, id, "Updated", 0).await, id);
    assert_eq!(
        request(&socket, json!({"command":"list"}))["notifications"][0]["summary"],
        "Updated"
    );

    assert_eq!(
        request(
            &socket,
            json!({"command":"action", "id":id, "key":"missing"})
        )["ok"],
        false
    );
    assert_eq!(
        request(&socket, json!({"command":"action", "id":id, "key":"open"}))["ok"],
        true
    );
    let action = tokio::time::timeout(Duration::from_secs(2), invoked.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        action.body().deserialize::<(u32, String)>().unwrap(),
        (id, "open".into())
    );
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(close.body().deserialize::<(u32, u32)>().unwrap(), (id, 2));
    assert!(
        tokio::time::timeout(Duration::from_millis(60), foreign_signals.next())
            .await
            .is_err()
    );
    let listed = request(&socket, json!({"command":"list"}));
    assert_eq!(listed["notifications"][0]["active"], false);
    assert_eq!(listed["notifications"][0]["actions"], json!([]));

    let expired_id = notify(&proxy, 0, "Brief", 10).await;
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        close.body().deserialize::<(u32, u32)>().unwrap(),
        (expired_id, 1)
    );
    let withdrawn_id = notify(&proxy, 0, "Withdraw", 0).await;
    let _: () = proxy
        .call("CloseNotification", &(withdrawn_id,))
        .await
        .unwrap();
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        close.body().deserialize::<(u32, u32)>().unwrap(),
        (withdrawn_id, 3)
    );
    let dismissed_id = notify(&proxy, 0, "Dismiss", 0).await;
    assert_eq!(
        request(&socket, json!({"command":"dismiss", "id":dismissed_id}))["ok"],
        true
    );
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        close.body().deserialize::<(u32, u32)>().unwrap(),
        (dismissed_id, 2)
    );

    // Another process cannot take ownership or delete the live control socket.
    let mut duplicate = OwnedChild {
        child: Command::new(env!("CARGO_BIN_EXE_hearthspace-notifications"))
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
        group: false,
    };
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = duplicate.child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "duplicate daemon did not refuse the owned bus name"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(request(&socket, json!({"command":"list"}))["ok"], true);
}

async fn notify(proxy: &Proxy<'_>, replaces_id: u32, summary: &str, expiry: i32) -> u32 {
    let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    proxy
        .call(
            "Notify",
            &(
                "Test application",
                replaces_id,
                "",
                summary,
                "Plain <b>text</b>",
                vec!["open", "Open"],
                hints,
                expiry,
            ),
        )
        .await
        .unwrap()
}

fn request(socket: &Path, value: Value) -> Value {
    let response = hearthspace_ipc::transport::request_line(
        socket,
        &format!("{value}\n"),
        Duration::from_millis(500),
    )
    .unwrap();
    serde_json::from_str(&response).unwrap()
}

struct OwnedChild {
    child: Child,
    group: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.group {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        } else {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
