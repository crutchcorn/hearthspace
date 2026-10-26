//! Real libpam transport tests. Only the root-owned namespace runner may enable
//! them; ordinary `cargo test`, including `--ignored`, cannot use host PAM.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use nix::unistd::{User, geteuid, getuid};

use super::{Conversation, Failure, Identity, PAM_POLICY, PAM_SERVICE, authenticate};
use pam_client::{Context, Flag};
use zeroize::Zeroizing;

const DIRECTORY: &str = "/etc/pam.d";
const MARKER: &str = "/etc/pam.d/.hearthspace-lock-test";
const EVENTS: &str = "/etc/pam.d/.events";
const SPOOFED_USER: &str = "hearthspace-lock-spoofed-environment-user";
const SYNTHETIC: &str = "hearthspace private PAM fixture credential";

fn trusted(path: &Path, directory: bool) -> fs::Metadata {
    let metadata = fs::symlink_metadata(path).expect("fixture must exist");
    assert_eq!(metadata.uid(), 0, "fixture must belong to root");
    assert_eq!(metadata.mode() & 0o022, 0, "fixture must not be writable");
    assert_eq!(metadata.is_dir(), directory);
    assert!(directory || metadata.is_file(), "no fixture symlinks");
    metadata
}

// This guard runs before Identity resolution or any call into PAM, including
// the missing-policy positive control. A test-only environment value alone is
// insufficient: require a protected marker, a different namespace, private
// propagation and the actual temporary directory mounted at the fixed path.
fn guarded_case() -> String {
    trusted(Path::new(MARKER), false);
    let marker = fs::read_to_string(MARKER).expect("runner marker");
    let fields: Vec<_> = marker.lines().collect();
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[0], "hearthspace-lock-private-pam-v1");
    assert_eq!(
        fields[1],
        std::env::var("HEARTHSPACE_LOCK_TEST_NONCE").expect("use lock-auth-test")
    );
    let namespace = fs::read_link("/proc/self/ns/mnt").unwrap();
    assert_eq!(namespace.to_str().unwrap(), fields[3]);
    assert_ne!(fields[2], fields[3], "must leave the parent's namespace");
    let uid = fields[4].parse::<u32>().unwrap();
    assert_ne!(
        uid, 0,
        "authentication must run as the intended non-root user"
    );
    assert_eq!(getuid().as_raw(), uid);
    assert_eq!(geteuid().as_raw(), uid);

    let source = Path::new(fields[6]);
    assert!(fields[6].starts_with("/tmp/hearthspace-lock-pam-"));
    assert_eq!(source.file_name().unwrap(), "pam.d");
    trusted(source.parent().unwrap(), true);
    let original = trusted(source, true);
    let mounted = trusted(Path::new(DIRECTORY), true);
    assert_eq!(
        (original.dev(), original.ino()),
        (mounted.dev(), mounted.ino())
    );
    // Linux-PAM also consults its vendor directory for a missing service.
    // Keep that fallback isolated even if the production preflight regresses.
    let vendor = Path::new("/usr/lib/pam.d");
    if vendor.exists() {
        let mounted = trusted(vendor, true);
        assert_eq!(
            (original.dev(), original.ino()),
            (mounted.dev(), mounted.ino())
        );
    }
    let mounts = fs::read_to_string("/proc/self/mountinfo").unwrap();
    let mut found_bind_mount = false;
    for mount in mounts.lines() {
        let columns: Vec<_> = mount.split_ascii_whitespace().collect();
        let separator = columns.iter().position(|column| *column == "-").unwrap();
        for option in &columns[6..separator] {
            assert!(
                !option.starts_with("shared:")
                    && !option.starts_with("master:")
                    && !option.starts_with("propagate_from:"),
                "mount propagation must be private"
            );
        }
        found_bind_mount |= columns[4] == DIRECTORY;
    }
    assert!(
        found_bind_mount,
        "fixed PAM directory must be a separate mount"
    );
    trusted(
        Path::new("/etc/pam.d/pam_hearthspace_lock_fixture.so"),
        false,
    );
    trusted(Path::new("/etc/pam.d/other"), false);
    assert_eq!(std::env::var("USER").unwrap(), SPOOFED_USER);
    assert_eq!(std::env::var("LOGNAME").unwrap(), SPOOFED_USER);
    assert!(
        events().is_empty(),
        "each case starts with an empty journal"
    );
    fields[5].to_owned()
}

fn events() -> Vec<String> {
    fs::read_to_string(EVENTS)
        .expect("fixture journal")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn expected_events(case: &str) -> Vec<&'static str> {
    if matches!(
        case,
        "missing_policy" | "unreadable_policy" | "invalid_owner" | "invalid_mode"
    ) {
        return vec![];
    }
    let mut events = vec!["auth", "auth-flags", "uid-user", "echo-off"];
    if case == "wrong" {
        events.push("credential-rejected");
        return events;
    }
    events.push("credential-accepted");
    let rejection = match case {
        "auth_userchange" => Some("auth-user-change"),
        "second_prompt" => Some("second-prompt-rejected"),
        "echo_prompt" => Some("echo-prompt-rejected"),
        "error_then_success" => Some("error-message"),
        _ => None,
    };
    if let Some(event) = rejection {
        events.extend([event, "auth-success"]);
        return events;
    }
    events.extend(["auth-success", "account", "account-flags"]);
    match case {
        "success" => events.push("account-success"),
        "expired" => events.push("account-expired"),
        "new_authtok" => events.push("account-new-authtok"),
        "account_userchange" => events.extend(["account-user-change", "account-success"]),
        _ => panic!("unknown private PAM case"),
    }
    events
}

#[test]
#[ignore = "only via sudo python3 scripts/lock-auth-test with a prebuilt test binary"]
fn real_pam_private_policy() {
    let case = guarded_case();
    let identity = Identity::current().expect("real UID must resolve through NSS");
    let user = User::from_uid(getuid()).unwrap().unwrap();
    assert_eq!(identity.username(), user.name);
    assert_ne!(identity.username(), SPOOFED_USER);

    if case == "unreadable_policy" {
        let metadata = trusted(Path::new(PAM_POLICY), false);
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(
            fs::File::open(PAM_POLICY).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }

    let expected = match case.as_str() {
        "success" => Ok(()),
        "missing_policy" | "unreadable_policy" | "invalid_owner" | "invalid_mode" => {
            Err(Failure::Unavailable)
        }
        "wrong" | "expired" | "new_authtok" | "auth_userchange" | "account_userchange"
        | "second_prompt" | "echo_prompt" | "error_then_success" => Err(Failure::Denied),
        _ => panic!("unknown private PAM case"),
    };
    let credential = if case == "wrong" {
        "deliberately incorrect synthetic credential"
    } else {
        SYNTHETIC
    };
    assert_eq!(
        authenticate(&identity, Zeroizing::new(credential.into())),
        expected
    );
    assert_eq!(events(), expected_events(&case));

    if matches!(case.as_str(), "missing_policy" | "unreadable_policy") {
        if case == "missing_policy" {
            assert!(!Path::new(PAM_POLICY).exists());
        }
        // Prove `other` actually prompts and accepts the synthetic credential.
        // pam_permit alone is not a useful control: no prompt already fails the
        // production conversation check. Only this guarded test bypasses the
        // policy-file preflight, using the same PAM service and conversation.
        let conversation = Conversation::new(Zeroizing::new(SYNTHETIC.into())).unwrap();
        let mut pam = Context::new(PAM_SERVICE, Some(identity.username()), conversation).unwrap();
        pam.authenticate(Flag::DISALLOW_NULL_AUTHTOK).unwrap();
        pam.acct_mgmt(Flag::NONE).unwrap();
        assert!(pam.conversation().accepted());
        assert_eq!(pam.user().unwrap(), identity.username());
        assert_eq!(events(), expected_events("success"));
    }
    // The runner checks this receipt too: a stale binary matching zero tests
    // must not produce a false pass from libtest's successful exit status.
    fs::write("/etc/pam.d/.completed", format!("{case}\n")).unwrap();
}
