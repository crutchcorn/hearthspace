use super::*;
use serde_json::Value;
use std::os::unix::fs::symlink;

struct Fixture {
    _root: tempfile::TempDir,
    dependency: Dependency,
    receipt: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dependency = Dependency::beside(&root.path().join("hearthspace-compositor")).unwrap();
        // If validation executes this script, it leaves observable evidence.
        let script = format!(
            "#!/bin/sh\n: > '{}'\nexit 91\n",
            root.path().join("executed").display()
        );
        fs::write(&dependency.binary, script.as_bytes()).unwrap();
        fs::set_permissions(&dependency.binary, fs::Permissions::from_mode(0o700)).unwrap();
        let mut receipt: Value = serde_json::from_str(REQUIREMENTS).unwrap();
        receipt["sha256"] = format!("{:x}", Sha256::digest(script.as_bytes())).into();
        let result = Self {
            _root: root,
            dependency,
            receipt,
        };
        result.save_receipt();
        result
    }

    fn save_receipt(&self) {
        fs::write(
            &self.dependency.receipt,
            serde_json::to_vec(&self.receipt).unwrap(),
        )
        .unwrap();
    }

    fn rejects(&self, expected: &str) {
        let error = self.dependency.validate().unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(
            error.contains(self.dependency.binary.to_str().unwrap()),
            "{error}"
        );
        assert!(
            error.contains(self.dependency.receipt.to_str().unwrap()),
            "{error}"
        );
        assert!(error.contains("normal user"), "{error}");
    }
}

#[test]
fn accepts_exact_receipt_and_bytes_without_executing_the_binary() {
    let fixture = Fixture::new();
    fixture.dependency.validate().unwrap();
    assert!(!fixture._root.path().join("executed").exists());
}

#[test]
fn missing_binary_fails_with_both_paths_and_preparation_instructions() {
    let fixture = Fixture::new();
    fs::remove_file(&fixture.dependency.binary).unwrap();
    fixture.rejects("No such file");
}

#[test]
fn missing_receipt_cannot_be_replaced_by_a_binary_alone() {
    let fixture = Fixture::new();
    fs::remove_file(&fixture.dependency.receipt).unwrap();
    fixture.rejects("No such file");
}

#[test]
fn malformed_or_incomplete_receipt_fails_closed() {
    let fixture = Fixture::new();
    fs::write(&fixture.dependency.receipt, b"{").unwrap();
    fixture.rejects("EOF");
    fs::write(&fixture.dependency.receipt, b"{}").unwrap();
    fixture.rejects("missing field");
}

#[test]
fn exact_requirements_reject_newer_versions_formats_and_other_patches() {
    for (field, value) in [
        ("format_version", Value::from(2)),
        ("version", Value::from("24.1.11")),
        ("source_archive_sha256", Value::from("f".repeat(64))),
        ("surface_input_targets_sha256", Value::from("f".repeat(64))),
    ] {
        let mut fixture = Fixture::new();
        fixture.receipt[field] = value;
        fixture.save_receipt();
        fixture.rejects("exact embedded");
    }
}

#[test]
fn tampering_after_success_is_rejected_on_revalidation() {
    let fixture = Fixture::new();
    fixture.dependency.validate().unwrap();
    fs::write(&fixture.dependency.binary, b"changed after preflight").unwrap();
    fixture.rejects("SHA-256");
}

#[test]
fn symlinked_binary_is_rejected_even_when_hash_matches() {
    let fixture = Fixture::new();
    let target = fixture.dependency.binary.with_extension("real");
    fs::rename(&fixture.dependency.binary, &target).unwrap();
    symlink(target, &fixture.dependency.binary).unwrap();
    fixture.rejects("nonsymlink");
}

#[test]
fn symlinked_receipt_is_rejected_even_when_its_contents_match() {
    let fixture = Fixture::new();
    let target = fixture.dependency.receipt.with_extension("real");
    fs::rename(&fixture.dependency.receipt, &target).unwrap();
    symlink(target, &fixture.dependency.receipt).unwrap();
    fixture.rejects("nonsymlink");
}

#[test]
fn directories_and_non_executable_binaries_are_rejected() {
    let fixture = Fixture::new();
    fs::set_permissions(
        &fixture.dependency.binary,
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fixture.rejects("executable");
    fs::remove_file(&fixture.dependency.binary).unwrap();
    fs::create_dir(&fixture.dependency.binary).unwrap();
    fixture.rejects("regular");
}

#[test]
fn companion_resolution_never_uses_a_relative_directory() {
    assert!(Dependency::beside(Path::new("./hearthspace-compositor")).is_err());
}

#[test]
fn complete_receipt_at_limit_is_valid_but_any_extra_byte_is_rejected() {
    let fixture = Fixture::new();
    let mut bytes = serde_json::to_vec(&fixture.receipt).unwrap();
    bytes.resize(RECEIPT_LIMIT, b' ');
    fs::write(&fixture.dependency.receipt, &bytes).unwrap();
    fixture.dependency.validate().unwrap();
    bytes.push(b' ');
    fs::write(&fixture.dependency.receipt, &bytes).unwrap();
    fixture.rejects("exceeds 65536 bytes");
}

#[test]
fn padding_cannot_hide_trailing_garbage_beyond_receipt_read_limit() {
    let fixture = Fixture::new();
    let mut bytes = serde_json::to_vec(&fixture.receipt).unwrap();
    bytes.resize(RECEIPT_LIMIT, b' ');
    bytes.extend_from_slice(b"not JSON");
    fs::write(&fixture.dependency.receipt, bytes).unwrap();
    fixture.rejects("exceeds 65536 bytes");
}

#[test]
fn trailing_garbage_within_limit_is_also_rejected() {
    let fixture = Fixture::new();
    let mut bytes = serde_json::to_vec(&fixture.receipt).unwrap();
    bytes.extend_from_slice(b" not JSON");
    fs::write(&fixture.dependency.receipt, bytes).unwrap();
    fixture.rejects("trailing characters");
}
