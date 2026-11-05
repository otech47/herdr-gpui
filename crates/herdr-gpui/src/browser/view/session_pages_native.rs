use crate::HerdrWindow;
use crate::browser::{Pick, annotate::Rect, snapshot};
use gpui::{Context, WindowHandle};
use std::{path::PathBuf, time::Duration};

impl HerdrWindow {
    pub(crate) fn check_native_session_page(
        handle: WindowHandle<Self>,
        path: PathBuf,
        cx: &mut gpui::App,
    ) {
        cx.spawn(async move |cx| {
            for _ in 0..150 {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if handle
                    .update(cx, |this, window, cx| {
                        this.poll_browser(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let result =
                handle.update(cx, |this, _, cx| this.capture_native_session_page(path, cx));
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("SESSION PAGE native FAIL: {result:?}");
            }
            if std::env::var_os("HERDR_TEST_MARKDOWN_FLOW").is_some() {
                for (label, script) in [
                    ("open", "document.querySelector('a[href=\"details.md\"]').click(); true"),
                    ("link", "const linked = document.querySelector('h1')?.textContent === 'Report details'; history.back(); linked"),
                    ("back", "document.querySelector('h1')?.textContent === 'Session pages'"),
                ] {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    let result = handle.update(cx, |this, _, cx| {
                        let Some(Pick::Page(id)) = this.active_group().and_then(|group| this.group_pick(group)) else {
                            anyhow::bail!("fixture lost its page");
                        };
                        let page = this.browser.pages.page(id).ok_or_else(|| anyhow::anyhow!("fixture lost its web view"))?;
                        page.read(cx).raw().evaluate_script_with_callback(script, move |value| {
                            eprintln!("SESSION PAGE native markdown {label}: {value}");
                        }).map_err(|error| anyhow::anyhow!("{error}"))?;
                        Ok(())
                    });
                    if !matches!(result, Ok(Ok::<(), anyhow::Error>(()))) {
                        eprintln!("SESSION PAGE native FAIL: {result:?}");
                    }
                }
            }
        })
        .detach();
    }

    fn capture_native_session_page(
        &self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let Some(Pick::Page(id)) = self.active_group().and_then(|group| self.group_pick(group))
        else {
            anyhow::bail!("fixture did not select a page");
        };
        let Some(page) = self.browser.pages.page(id) else {
            anyhow::bail!("fixture has no native web view");
        };
        page.read(cx).raw().evaluate_script_with_callback(
            "document.querySelector('#review')?.click(); JSON.stringify({title:document.title, result:document.querySelector('#status')?.textContent, markdown:!!document.querySelector('.markdown-body'), heading:document.querySelector('h1')?.textContent, table:!!document.querySelector('table'), code:!!document.querySelector('pre code'), images:[...document.images].every(i=>i.complete&&i.naturalWidth>0), overflow:document.documentElement.scrollWidth>innerWidth})",
            |value| eprintln!("SESSION PAGE native content: {value}"),
        ).map_err(|error| anyhow::anyhow!("{error}"))?;
        let Some(bounds) = self.browser.page_bounds.borrow().get(&id).copied() else {
            anyhow::bail!("fixture page did not draw");
        };
        let rect = Rect {
            x: 0.,
            y: 0.,
            width: f64::from(f32::from(bounds.size.width)),
            height: f64::from(f32::from(bounds.size.height)),
        };
        let executor = cx.background_executor().clone();
        snapshot::capture(page.read(cx).raw(), rect, move |tiff| {
            executor
                .spawn(async move {
                    let result = tiff
                        .and_then(|bytes| snapshot::png(&bytes))
                        .ok_or_else(|| std::io::Error::other("WebKit returned no page image"))
                        .and_then(|png| std::fs::write(path, png));
                    eprintln!("SESSION PAGE native snapshot: {result:?}");
                })
                .detach();
        });
        Ok(())
    }
}
