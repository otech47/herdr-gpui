use super::*;

#[test]
fn launcher_chords_run_without_the_prefix() {
    let keymap = Keymap::default();
    let commands = [
        custom("tools", &["ctrl+cmd+q"]),
        custom("fedi", &["ctrl+cmd+w"]),
    ];
    let find = |typed: &str| {
        keymap
            .custom_command(&commands, &keystroke(typed), false)
            .map(|command| command.command_id.as_str())
    };
    assert_eq!(find("ctrl-cmd-q"), Some("tools"));
    assert_eq!(find("ctrl-cmd-w"), Some("fedi"));
    assert_eq!(find("cmd-w"), None);
    assert_eq!(find("cmd-q"), None);
    assert_eq!(keymap.custom_labels(&commands[0]), ["ctrl-cmd-q"]);
}
