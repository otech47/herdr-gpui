#![allow(clippy::unwrap_used)]

use super::*;
use std::{
    process::Command,
    sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

fn local_shell() -> Result<Shell> {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-s")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    Shell::start(command)
}

fn shared(target: &str, kind: MonitorKind) -> SharedShell {
    SharedShell::connect_with(target, kind, local_shell).unwrap()
}

#[test]
fn one_host_and_monitor_kind_share_a_shell() {
    let first = shared("studio-shared", MonitorKind::SystemLoad);
    let second = shared("studio-shared", MonitorKind::SystemLoad);
    assert!(Arc::ptr_eq(&first.shell, &second.shell));
    let output = first.run("printf shared", Duration::from_secs(2)).unwrap();
    assert_eq!(output.stdout, "shared");
}

#[test]
fn monitor_kinds_and_hosts_keep_separate_shells() {
    let load = shared("studio-separate", MonitorKind::SystemLoad);
    let ports = shared("studio-separate", MonitorKind::ListeningPorts);
    let mini = shared("mini-separate", MonitorKind::SystemLoad);
    assert!(!Arc::ptr_eq(&load.shell, &ports.shell));
    assert!(!Arc::ptr_eq(&load.shell, &mini.shell));
}

#[test]
fn concurrent_first_connects_dial_once() {
    const WORKERS: usize = 5;
    let barrier = Arc::new(Barrier::new(WORKERS));
    let dials = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..WORKERS)
        .map(|_| {
            let barrier = barrier.clone();
            let dials = dials.clone();
            thread::spawn(move || {
                barrier.wait();
                SharedShell::connect_with("studio-concurrent", MonitorKind::SystemLoad, || {
                    dials.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(50));
                    local_shell()
                })
                .unwrap()
            })
        })
        .collect();
    let shells: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(dials.load(Ordering::SeqCst), 1);
    assert!(
        shells
            .iter()
            .all(|shell| Arc::ptr_eq(&shell.shell, &shells[0].shell))
    );
}

#[test]
fn concurrent_runs_keep_each_callers_output() {
    const WORKERS: usize = 4;
    let shell = shared("studio-concurrent-runs", MonitorKind::Monitoring);
    let barrier = Arc::new(Barrier::new(WORKERS));
    let handles: Vec<_> = (0..WORKERS)
        .map(|index| {
            let shell = shell.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                shell
                    .run(&format!("printf output-{index}"), Duration::from_secs(2))
                    .unwrap()
                    .stdout
            })
        })
        .collect();
    let outputs: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(outputs, ["output-0", "output-1", "output-2", "output-3"]);
}

#[test]
fn a_broken_shell_is_not_reused() {
    let first = shared("studio-broken", MonitorKind::SystemLoad);
    assert!(first.run("exit 0", Duration::from_secs(2)).is_err());
    let second = shared("studio-broken", MonitorKind::SystemLoad);
    assert!(!Arc::ptr_eq(&first.shell, &second.shell));
}

#[test]
fn a_stale_holder_does_not_evict_its_replacement() {
    let first = shared("studio-stale", MonitorKind::AgentModels);
    assert!(first.run("exit 0", Duration::from_secs(2)).is_err());
    let second = shared("studio-stale", MonitorKind::AgentModels);
    assert!(first.run("printf stale", Duration::from_secs(2)).is_err());
    let third = shared("studio-stale", MonitorKind::AgentModels);
    assert!(Arc::ptr_eq(&second.shell, &third.shell));
}

#[test]
fn the_last_holder_releases_the_pool_entry() {
    let dials = AtomicUsize::new(0);
    let first = SharedShell::connect_with("studio-teardown", MonitorKind::ListeningPorts, || {
        dials.fetch_add(1, Ordering::SeqCst);
        local_shell()
    })
    .unwrap();
    let weak = Arc::downgrade(&first.shell);
    drop(first);
    assert!(weak.upgrade().is_none());
    let second = SharedShell::connect_with("studio-teardown", MonitorKind::ListeningPorts, || {
        dials.fetch_add(1, Ordering::SeqCst);
        local_shell()
    })
    .unwrap();
    assert_eq!(dials.load(Ordering::SeqCst), 2);
    assert!(second.run("printf fresh", Duration::from_secs(2)).is_ok());
}
