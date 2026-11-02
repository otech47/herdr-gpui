use super::*;

impl HerdrWindow {
    pub(crate) fn preview_agent_sessions(
        &mut self,
        mode: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled: Vec<_> = self.endpoints.iter().map(|e| e.enabled).collect();
        for endpoint in &mut self.endpoints {
            endpoint.enabled = false;
        }
        self.open_agent_sessions(window, cx);
        for (endpoint, enabled) in self.endpoints.iter_mut().zip(enabled) {
            endpoint.enabled = enabled;
        }
        let Some(endpoint) = self.endpoints.first() else {
            return;
        };
        let Ok(host) = crate::teleport::host_for(&ConnectTarget::Local) else {
            return;
        };
        let source = Source {
            id: endpoint.id.clone(),
            label: "Mini".into(),
            generation: endpoint.generation,
            host,
            cli: vec!["herdr".into()],
        };
        let Some(picker) = &mut self.menu.agent_sessions else {
            return;
        };
        picker.rows = [
            (
                "Prepare the mobile release",
                "codex",
                "Jarvis",
                "/projects/mobile",
            ),
            (
                "Review navigation changes",
                "codex",
                "Roboz",
                "/projects/mobile",
            ),
            (
                "Fix the session picker",
                "claude",
                "Saved session",
                "/projects/herdr",
            ),
            (
                "Investigate the failing tests",
                "kimi",
                "Saved session",
                "/projects/api",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (title, agent, origin, cwd))| Row {
            host: source.clone(),
            entry: Entry {
                agent: agent.into(),
                session: format!("fixture-session-{index}"),
                title: title.into(),
                cwd: cwd.into(),
                source: origin.into(),
                modified: 0.,
            },
        })
        .collect();
        picker.filter("");
        if mode != "list" {
            self.watch_agent_session(0, window, cx);
            if let Some(watch) = self
                .menu
                .agent_sessions
                .as_mut()
                .and_then(|p| p.watch.as_mut())
            {
                watch.observation = Some(Observation {
                    state: if mode == "ready" { Activity::Ready } else { Activity::Watch },
                    output: "The release branch is ready. I am checking the Android build and the changes included in this release.\n\nWorking: run build checks\n\nThe navigation checks passed. The remaining build is still running.\n\nI will save the result when the checks finish.".into(),
                });
                watch.next = Instant::now() + Duration::from_secs(86400);
            }
        }
        cx.notify();
    }
}
