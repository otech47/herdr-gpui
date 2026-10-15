use super::*;

#[test]
fn launchers_are_chords_with_a_label_and_a_command() -> anyhow::Result<()> {
    let config = Config::parse(
        "[[launchers]]\nkey = 'ctrl-cmd-2'\nlabel = 'claude in tools'\ncommand = 'herdr-launch tools ~/tools claude'\n\
         [[launchers]]\nkey = 'ctrl-cmd-3'\ncommand = ' herdr-launch fedi ~/fedibtc/fedi claude '\nask_device = false\n",
    )?;
    assert_eq!(config.launchers.len(), 2);
    assert_eq!(config.launchers[0].label, "claude in tools");
    assert!(config.launchers[0].ask_device);
    assert_eq!(
        config.launchers[0].key,
        gpui::Keystroke::parse("ctrl-cmd-2")?
    );
    assert_eq!(
        config.launchers[1].label,
        "herdr-launch fedi ~/fedibtc/fedi claude"
    );
    assert_eq!(
        config.launchers[1].command,
        "herdr-launch fedi ~/fedibtc/fedi claude"
    );
    assert!(!config.launchers[1].ask_device);
    assert!(Config::parse("")?.launchers.is_empty());
    let extra =
        Config::parse("[[launchers]]\nkey = 'ctrl-cmd-2'\ncommand = 'true'\ndevice = 'mini'\n")?;
    assert_eq!(extra.launchers.len(), 1);
    assert!(
        extra.unknown_keys.iter().any(|key| key.contains("device")),
        "{:?}",
        extra.unknown_keys
    );
    Ok(())
}

#[test]
fn launcher_keys_need_a_free_modified_keystroke() {
    let refused = [
        ("[[launchers]]\nkey = '2'\ncommand = 'true'\n", "bare"),
        (
            "[[launchers]]\nkey = 'ctrl-cmd-a-b'\ncommand = 'true'\n",
            "invalid",
        ),
        ("[[launchers]]\nkey = 'cmd-t'\ncommand = 'true'\n", "bound"),
        (
            "[keybindings]\nnew_tab = 'ctrl-cmd-9'\n[[launchers]]\nkey = 'ctrl-cmd-9'\ncommand = 'true'\n",
            "bound by override",
        ),
        (
            "[[launchers]]\nkey = 'ctrl-cmd-2'\ncommand = 'true'\n[[launchers]]\nkey = 'cmd-ctrl-2'\ncommand = 'false'\n",
            "duplicate",
        ),
        (
            "[[launchers]]\nkey = 'ctrl-cmd-2'\ncommand = '  '\n",
            "empty",
        ),
    ];
    for (text, why) in refused {
        assert!(Config::parse(text).is_err(), "{why}: accepted {text:?}");
    }
    assert!(matches!(
        Config::parse("[[launchers]]\nkey = 'cmd-t'\ncommand = 'true'\n"),
        Err(Error::LauncherKeyTaken(key)) if key == "cmd-t"
    ));
    assert!(matches!(
        Config::parse("[[launchers]]\nkey = 'ctrl-cmd-a-b'\ncommand = 'true'\n"),
        Err(Error::InvalidLauncherKey { keystroke, .. }) if keystroke == "ctrl-cmd-a-b"
    ));
    assert!(matches!(
        Config::parse(
            "[[launchers]]\nkey = 'ctrl-cmd-2'\ncommand = 'true'\n[[launchers]]\nkey = 'cmd-ctrl-2'\ncommand = 'false'\n",
        ),
        Err(Error::DuplicateLauncherKey(key)) if key == "cmd-ctrl-2"
    ));
}
