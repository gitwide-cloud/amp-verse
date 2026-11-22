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
fn noncpu_request_fails_before_opening_runtime() {
    let out = worker().args(["--provider", "cuda"]).output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("CPU only"));
}
