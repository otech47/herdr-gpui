use super::*;

#[test]
fn new_tab_lists_spaces_and_defaults_its_agents() -> anyhow::Result<()> {
    assert!(Config::parse("")?.new_tab.is_none());
    let config = Config::parse(
        "[new_tab]\ncommand = '~/.local/bin/herdr-launch'\n\
         [[new_tab.spaces]]\nlabel = 'tools'\ndir = '~/tools'\n\
         [[new_tab.spaces]]\nlabel = 'fedi'\ndir = '~/fedibtc/fedi'\nwrap = ' nix develop -c '\n",
    )?;
    let new_tab = config
        .new_tab
        .ok_or_else(|| anyhow::anyhow!("no new_tab"))?;
    assert_eq!(new_tab.spaces.len(), 2);
    assert_eq!(new_tab.spaces[1].wrap, "nix develop -c");
    let keys: String = new_tab.agents.iter().map(|agent| agent.key).collect();
    assert_eq!(keys, "qwert");
    assert_eq!(new_tab.agents[0].command, "");
    let custom = Config::parse(
        "[new_tab]\ncommand = 'go'\n[[new_tab.agents]]\nkey = 'a'\n\
         [[new_tab.agents]]\nkey = 's'\ncommand = 'claude'\n",
    )?
    .new_tab
    .ok_or_else(|| anyhow::anyhow!("no new_tab"))?;
    assert_eq!(custom.agents[0].label, "Shell");
    assert_eq!(custom.agents[1].label, "claude");
    Ok(())
}

#[test]
fn new_tab_refuses_a_bad_key_space_or_command() {
    for (text, why) in [
        ("[new_tab]\n", "no command"),
        (
            "[new_tab]\ncommand = 'go'\n[[new_tab.agents]]\nkey = 'Q'\n",
            "uppercase key",
        ),
        (
            "[new_tab]\ncommand = 'go'\n[[new_tab.agents]]\nkey = '1'\n",
            "digit key",
        ),
        (
            "[new_tab]\ncommand = 'go'\n[[new_tab.agents]]\nkey = 'a'\n[[new_tab.agents]]\nkey = 'a'\n",
            "repeated key",
        ),
        (
            "[new_tab]\ncommand = 'go'\n[[new_tab.spaces]]\nlabel = 'tools'\n",
            "space without dir",
        ),
        (
            "[new_tab]\ncommand = 'go'\n[[new_tab.spaces]]\nlabel = 'a'\ndir = '~'\n[[new_tab.spaces]]\nlabel = 'a'\ndir = '~'\n",
            "repeated space",
        ),
    ] {
        assert!(Config::parse(text).is_err(), "{why}");
    }
}
