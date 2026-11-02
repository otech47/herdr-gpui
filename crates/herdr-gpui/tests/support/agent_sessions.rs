use super::{Isolated, Sandbox, daemon_binary};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Stdio,
    time::{Duration, Instant, SystemTime},
};

#[test]
#[ignore = "starts an explicitly selected isolated daemon and a synthetic agent"]
fn saved_session_opens_once_and_watch_cannot_start_the_agent() {
    let binary = daemon_binary();
    let mut isolated = Isolated {
        sandbox: Sandbox::new(),
        daemon: None,
        gui: None,
    };
    let root = &isolated.sandbox.dir;
    let bin = root.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("codex");
    fs::write(
        &fake,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$@\" >> \"$HOME/resumed\"\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display());
    isolated.daemon = Some(
        isolated
            .sandbox
            .command(&binary, "daemon.log")
            .env("PATH", &path)
            .arg("server")
            .spawn()
            .unwrap(),
    );
    isolated
        .sandbox
        .wait_for_daemon(isolated.daemon.as_mut().unwrap(), Duration::from_secs(20));
    let sid = "11111111-2222-3333-4444-555555555555";
    let cwd = root.join("checkout");
    fs::create_dir(&cwd).unwrap();
    let transcript = root.join(format!(
        ".codex/sessions/2026/11/02/rollout-test-{sid}.jsonl"
    ));
    fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    fs::write(&transcript, format!("{}\n{}\n",
        serde_json::json!({"type":"session_meta","payload":{"id":sid,"cwd":cwd,"thread_source":"user"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"Synthetic session"}}))).unwrap();
    fs::File::open(&transcript)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(100))
        .unwrap();
    let job = root.join("work/roboz/runtime/jarvis-jobs/task");
    fs::create_dir_all(&job).unwrap();
    fs::write(
        job.join("request.json"),
        r#"{"body":{"text":"Synthetic session"}}"#,
    )
    .unwrap();
    let set_state = |status: &str| {
        fs::write(job.join("status.json"), serde_json::json!({
        "status":status, "session_id":sid, "finished":if status == "completed" {Some(1)} else {None},
    }).to_string()).unwrap()
    };
    set_state("running");
    let helper = root.join("sessions.py");
    fs::write(
        &helper,
        include_str!("../../../../scripts/agent_sessions.py"),
    )
    .unwrap();
    let python = std::env::var_os("HERDR_TEST_PYTHON")
        .expect("set HERDR_TEST_PYTHON to an absolute Python 3 executable");
    let request = |operation: &str| -> serde_json::Value {
        let output = isolated
            .sandbox
            .command(&python, "helper.log")
            .env("PATH", &path)
            .arg(&helper)
            .arg(
                serde_json::json!({
                    "operation":operation,"herdr":[binary],"key":{"agent":"codex","session":sid}
                })
                .to_string(),
            )
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };
    assert_eq!(request("list")["entries"][0]["source"], "Jarvis");
    assert_eq!(request("watch")["state"], "watch");
    assert!(
        request("open")["error"]
            .as_str()
            .unwrap()
            .contains("still running")
    );
    assert!(!root.join("resumed").exists());
    set_state("completed");
    assert_eq!(request("watch")["state"], "ready");
    let first = request("open");
    assert!(first.get("error").is_none(), "{first}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("resumed").exists() {
        assert!(
            Instant::now() < deadline,
            "agent command was not delivered: {first}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let expected = format!("{}\nresume\n{sid}\n", cwd.display());
    assert_eq!(fs::read_to_string(root.join("resumed")).unwrap(), expected);
    assert_eq!(request("open"), first);
    assert_eq!(fs::read_to_string(root.join("resumed")).unwrap(), expected);
    eprintln!(
        "PASS: read-only watch; explicit continuation; original cwd and session; duplicate prevention"
    );
}
