use super::*;
use crate::Error;
use herdr_client::SavedHost;

fn parse(args: &[&str]) -> crate::Result<Args> {
    Args::parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn the_port_is_required_and_the_rest_default() {
    assert!(matches!(parse(&[]), Err(Error::Usage(_))));
    assert_eq!(
        parse(&["--port", "7480"]).unwrap(),
        Args {
            port: 7480,
            label: "Local".to_owned(),
            saved: false,
            ssh: Vec::new(),
        }
    );
}

#[test]
fn malformed_arguments_are_refused() {
    for args in [
        &["--port"][..],
        &["--port", "http"],
        &["--port", "70000"],
        &["--port", "1", "--ssh", "laptop"],
        &["--port", "1", "--ssh", "=laptop"],
        &["--port", "1", "--bind", "0.0.0.0"],
    ] {
        assert!(matches!(parse(args), Err(Error::Usage(_))), "{args:?}");
    }
}

#[test]
fn devices_keep_the_apps_ids_and_this_machine_leads() {
    let args = parse(&[
        "--port",
        "1",
        "--label",
        "Mini",
        "--ssh",
        "Laptop=me@laptop",
        "--ssh",
        "Again=box",
    ])
    .unwrap();
    let saved = vec![SavedHost {
        id: "box".to_owned(),
        label: "Box".to_owned(),
        target: "me@box".to_owned(),
        session: "work".to_owned(),
        enabled: false,
    }];
    let endpoints = args.endpoints(saved);
    let seen: Vec<_> = endpoints
        .iter()
        .map(|endpoint| {
            (
                endpoint.id.as_str(),
                endpoint.label.as_str(),
                endpoint.enabled,
            )
        })
        .collect();
    // A device named twice keeps its saved entry.
    assert_eq!(
        seen,
        [
            ("local", "Mini", true),
            ("ssh:box", "Box", false),
            ("ssh:me@laptop", "Laptop", true),
        ]
    );
    assert_eq!(
        endpoints[1].target,
        ConnectTarget::Ssh {
            target: "me@box".to_owned(),
            session: "work".to_owned(),
        }
    );
    assert_eq!(
        endpoints[2].target,
        ConnectTarget::Ssh {
            target: "me@laptop".to_owned(),
            session: "default".to_owned(),
        }
    );
}
