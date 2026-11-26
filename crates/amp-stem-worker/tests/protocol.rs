//! Actual native executable startup/IPC validation without a model or audio device.
use std::process::{Command, Stdio};
fn worker() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_amp-stem-worker"));
    command
        .args(["--model", "nonexistent-fixture.onnx"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
#[test]
fn clean_eof_exits_without_model_or_runtime() {
    let mut child = worker().spawn().unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
}
#[test]
fn malformed_header_exits_without_model_or_runtime() {
    use std::io::Write;
    let mut child = worker().spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&[b'x'; 1025])
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("header"));
}
#[test]
fn cuda_without_vendor_directory_fails_before_opening_runtime() {
    let out = worker().args(["--provider", "cuda"]).output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("--cuda-dll-dir"));
    assert!(!error.contains("Pinned local SCNet model missing"));
    assert!(!error.contains("ORT_DYLIB_PATH"));
}

#[test]
fn unsupported_provider_is_rejected_before_runtime_or_model_access() {
    let out = worker().args(["--provider", "dml"]).output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("supports cpu or cuda"));
    assert!(!error.contains("ORT_DYLIB_PATH"));
    assert!(!error.contains("Pinned local SCNet model missing"));
}
