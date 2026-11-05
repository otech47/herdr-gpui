use super::{FileLink, HerdrWindow, opened};
use crate::{
    browser::{LocalFile, Location, PageSource, Store, WebUrl},
    window::Flash,
};
use gpui::{Context, Window};
use herdr_client::{ConnectTarget, ScriptHost, ScriptLimits, run_script, shell_quote};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

pub(crate) fn preview_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "html" | "htm"
            )
        })
}

fn reveal_script(link: &FileLink) -> crate::Result<String> {
    let cwd = link.cwd.as_deref().unwrap_or("");
    if !link.path.starts_with('/') && !link.path.starts_with("~/") && !cwd.starts_with('/') {
        return Err(crate::Error::InvalidLocalPage);
    }
    Ok(format!(
        "path={}\ncase \"$path\" in '~/'*) path=\"$HOME/${{path#??}}\" ;; /*) ;; *) path={}/\"$path\" ;; esac\ntest -f \"$path\" && \"$HOME/.local/bin/tailopen\" --print \"$path\"",
        shell_quote(&link.path),
        shell_quote(cwd)
    ))
}

fn locate(link: FileLink, target: ConnectTarget) -> crate::Result<Location> {
    let host = match &target {
        ConnectTarget::Ssh { target, .. } => ScriptHost::Ssh(target),
        _ => {
            let home = crate::config::home().ok();
            let path = link
                .resolve(home.as_deref())
                .ok_or(crate::Error::InvalidLocalPage)?;
            let path = opened(&path)
                .filter(|path| preview_path(&path.to_string_lossy()))
                .ok_or(crate::Error::InvalidLocalPage)?;
            if let Ok(file) = LocalFile::new(&path, home.as_deref()) {
                return Ok(Location::Local { file });
            }
            ScriptHost::Local
        }
    };
    let mut output = Vec::new();
    run_script(
        host,
        &reveal_script(&link)?,
        std::io::empty(),
        &mut output,
        ScriptLimits {
            output: 8192,
            idle: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
    )?;
    let url = std::str::from_utf8(&output).map_err(|_| crate::Error::InvalidLocalPage)?;
    let url = WebUrl::try_from(url.trim())?;
    if url.report_title().is_none() {
        return Err(crate::Error::InvalidLocalPage);
    }
    Ok(Location::Web { url })
}

impl HerdrWindow {
    pub(crate) fn open_file_page(
        &mut self,
        link: FileLink,
        source: PageSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.endpoints[self.selected_endpoint]
            .connection
            .target
            .clone();
        let group = self.active_group();
        let selected = group.and_then(|group| self.group_pick(group));
        let work = cx
            .background_executor()
            .spawn(async move { locate(link, target) });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(location) => {
                    let current = this.browser_key()
                        == Some((source.scope.clone(), source.workspace.clone()))
                        && this.active_group() == group
                        && group.and_then(|group| this.group_pick(group)) == selected;
                    let id =
                        Store::update(cx, |store| store.open_session_location(&source, location));
                    if current && let Some(id) = id {
                        this.show_browser_tab_in(group, id, window, cx);
                    }
                    cx.notify();
                }
                Err(error) => this.show_flash(
                    Flash::warning(format!("Could not open document: {error}")),
                    cx,
                ),
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
