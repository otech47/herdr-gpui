use super::*;
use crate::Error;

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
fn this_machine_leads_the_named_devices() {
    let args = parse(&[
        "--port",
        "1",
        "--label",
        "Mini",
        "--ssh",
        "Laptop=me@laptop",
        "--ssh",
        "Again=me@laptop",
    ])
    .unwrap();
    assert_eq!(
        crate::devices::named(&args),
        [
            Endpoint {
                id: "local".to_owned(),
                label: "Mini".to_owned(),
                enabled: true,
                route: Route::Local,
            },
            Endpoint {
                id: "ssh:me@laptop".to_owned(),
                label: "Laptop".to_owned(),
                enabled: true,
                route: Route::Ssh {
                    target: "me@laptop".to_owned(),
                    session: "default".to_owned(),
                },
            },
        ]
    );
}
