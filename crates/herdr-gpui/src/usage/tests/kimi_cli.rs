use super::*;
use crate::usage::probe::Secret;
use secrecy::SecretString;

#[test]
fn toml_settings_read_nested_strings_only() {
    let mut exec = Exec::Local;
    let mut jar = CookieJar::default();
    let probe = Probe::new(&mut exec, provider("kimi"), None, &mut jar, Consent::Quiet);
    let config = Secret::from(SecretString::from(
        "[providers.\"managed:kimi-code\"]\n\
         type = \"kimi\"\n\
         api_key = \"\"\n\
         base_url = \"https://api.kimi.ai/coding/v1\"\n\
         retries = 3\n\
         [providers.\"managed:kimi-code\".oauth]\n\
         storage = \"file\"\n\
         key = \" oauth/kimi-code-env-0e4f99c69cc27850 \"\n",
    ));
    let provider = ["providers", "managed:kimi-code"];
    let text = |path: &[&str]| probe.toml_text(&config, &[&provider[..], path].concat());
    assert_eq!(
        text(&["base_url"]).as_deref(),
        Some("https://api.kimi.ai/coding/v1")
    );
    assert_eq!(text(&["oauth", "storage"]).as_deref(), Some("file"));
    assert_eq!(
        text(&["oauth", "key"]).as_deref(),
        Some("oauth/kimi-code-env-0e4f99c69cc27850")
    );
    // Empty, missing, non-string and non-table steps read as absent.
    assert_eq!(text(&["api_key"]), None);
    assert_eq!(text(&["oauth_host"]), None);
    assert_eq!(text(&["retries"]), None);
    assert_eq!(text(&["type", "oauth"]), None);
    let broken = Secret::from(SecretString::from("[providers\nkey = "));
    assert_eq!(probe.toml_text(&broken, &["providers", "key"]), None);
}
