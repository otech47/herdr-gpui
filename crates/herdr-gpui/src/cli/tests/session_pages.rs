use super::*;

#[test]
fn a_forwarded_report_names_its_device_without_starting_the_gui() {
    assert_eq!(
        LaunchOptions::parse([
            "browser",
            "open",
            "https://host.test/report.html",
            "--no-focus",
            "--workspace",
            "w1",
            "--device",
            "Mini"
        ])
        .unwrap()
        .mode,
        LaunchMode::Browser(BrowserCommand::Open {
            target: "https://host.test/report.html".into(),
            workspace: Some("w1".into()),
            device: Some("Mini".into()),
            focus: false
        })
    );
    assert!(matches!(
        LaunchOptions::parse(["browser", "open", "https://host.test/", "--device"]),
        Err(CliError::MissingDevice)
    ));
    assert!(matches!(
        LaunchOptions::parse([
            "browser",
            "open",
            "https://host.test/",
            "--device",
            "--no-focus"
        ]),
        Err(CliError::MissingDevice)
    ));
}
