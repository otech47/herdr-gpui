use super::*;

const DEVICES: [(&str, &str); 4] = [
    ("local", "Local"),
    ("ssh:93d87f03a0dd2de8c4df77aa633270d0", "Mini"),
    (
        "ssh:c8678aa217fbfcd65bb63f367fa3b0fa",
        "Devbox contributors",
    ),
    ("ssh:b5b318656ee02cc992706a4dae809c32", "Studio"),
];

fn order(names: &[&str]) -> Vec<usize> {
    let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    ordered(&names, DEVICES)
}

#[test]
fn no_order_keeps_the_catalog_order() {
    assert_eq!(order(&[]), [0, 1, 2, 3]);
}

#[test]
fn named_devices_lead_in_the_given_order() {
    assert_eq!(order(&["studio"]), [3, 0, 1, 2]);
    assert_eq!(order(&["Studio", "local", "Mini"]), [3, 0, 1, 2]);
    assert_eq!(order(&["Mini", "Studio"]), [1, 3, 0, 2]);
}

#[test]
fn devices_match_by_id_and_unknown_names_are_ignored() {
    assert_eq!(
        order(&["b5b318656ee02cc992706a4dae809c32", "Laptop", "local"]),
        [3, 0, 1, 2]
    );
    assert_eq!(
        order(&["ssh:93d87f03a0dd2de8c4df77aa633270d0"]),
        [1, 0, 2, 3]
    );
}
