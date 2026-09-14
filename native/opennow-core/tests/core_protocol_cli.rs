use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn hello(version: u32) -> Value {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("opennow-protocol-{version}-{unique}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_opennow-core"))
        .arg("--data-dir")
        .arg(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        json!({
            "type":"request","id":"hello","method":"core.hello",
            "params":{"protocolVersion":version,"shell":"qt"}
        })
    )
    .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        sender.send(result).unwrap();
    });
    let response = receiver.recv_timeout(Duration::from_secs(10));
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    if directory.exists() {
        std::fs::remove_dir_all(directory).unwrap();
    }
    serde_json::from_str(&response.expect("core handshake timed out").unwrap()).unwrap()
}

#[test]
fn protocol_three_shells_are_rejected_before_the_paged_library_contract() {
    let response = hello(3);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "incompatible_protocol");
    assert!(response.get("result").is_none());
}

#[test]
fn protocol_four_shells_receive_the_paged_library_capabilities() {
    let response = hello(4);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "incompatible_protocol");
}

#[test]
fn protocol_five_shells_receive_the_paged_library_capabilities() {
    let response = hello(5);
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["protocolVersion"], 5);
    let capabilities = response["result"]["capabilities"].as_array().unwrap();
    for capability in [
        "catalog.libraryPages.v1",
        "catalog.metadata.v1",
        "account.syncObservation.v1",
        "catalog.languages.v1",
    ] {
        assert!(capabilities.contains(&json!(capability)));
    }
}
