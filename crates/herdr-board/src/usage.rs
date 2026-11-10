//! Plan usage in the terms a gauge shows: which limit window, how much of it
//! is used, how alarming that is and when it resets.

use std::time::{Duration, SystemTime};

/// Which limit a window measures. A named window is one the service names
/// itself, such as a weekly limit scoped to one model.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Session,
    Daily,
    Weekly,
    Monthly,
    Named(String),
}

impl Kind {
    /// `5h`, `wk`, or the first word of a service's own name: what fits
    /// beside a meter in the status bar.
    pub fn short(&self) -> &str {
        match self {
            Self::Session => "5h",
            Self::Daily => "day",
            Self::Weekly => "wk",
            Self::Monthly => "mo",
            Self::Named(name) => name.split_whitespace().next().unwrap_or(name),
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Session => "Session",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Named(name) => name,
        }
    }

    /// How long the window usually runs, for judging its pace.
    pub fn length(&self) -> Option<Duration> {
        match self {
            Self::Session => Some(SESSION),
            Self::Daily => Some(DAY),
            Self::Weekly => Some(WEEK),
            Self::Monthly => Some(MONTH),
            Self::Named(_) => None,
        }
    }
}

pub const DAY: Duration = Duration::from_secs(86_400);
/// A calendar month varies; thirty days is close enough for a pace.
pub const MONTH: Duration = Duration::from_secs(30 * 86_400);
pub const SESSION: Duration = Duration::from_secs(5 * 3600);
pub const WEEK: Duration = Duration::from_secs(7 * 86_400);

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: Kind,
    /// Percent of the window used, clamped to 0..=100.
    pub used: f32,
    pub resets_at: Option<SystemTime>,
    /// How long the window lasts, when known, so its pace can be judged.
    pub length: Option<Duration>,
}

/// How a window's use compares with an even spend across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pace {
    /// Percent an even spend would have used by now.
    pub expected: f32,
    /// When the window runs out at the rate used so far, if before it resets.
    pub runs_out: Option<Duration>,
}

impl Window {
    pub fn new(
        kind: Kind,
        used: f64,
        resets_at: Option<SystemTime>,
        length: Option<Duration>,
    ) -> Self {
        Self {
            kind,
            used: used.clamp(0., 100.) as f32,
            resets_at,
            length,
        }
    }

    pub fn percent(&self) -> u32 {
        self.used.round() as u32
    }

    pub fn left(&self) -> u32 {
        100 - self.percent().min(100)
    }

    pub fn resets_in(&self, now: SystemTime) -> Option<Duration> {
        self.resets_at
            .map(|at| at.duration_since(now).unwrap_or_default())
    }

    /// `Session: 10% used, resets in 2h 53m`, one line of the bar's tooltip.
    pub fn describe(&self, now: SystemTime) -> String {
        let (title, percent) = (self.kind.title(), self.percent());
        match self.resets_in(now) {
            Some(left) => format!("{title}: {percent}% used, resets in {}", countdown(left)),
            None => format!("{title}: {percent}% used"),
        }
    }

    /// Nothing until a hundredth of the window has passed: an early estimate
    /// swings too far to be useful.
    pub fn pace(&self, now: SystemTime) -> Option<Pace> {
        let length = self.length?;
        let left = self.resets_at?.duration_since(now).ok()?;
        let elapsed = length.checked_sub(left)?;
        if elapsed < length / 100 {
            return None;
        }
        let expected = (elapsed.as_secs_f64() / length.as_secs_f64() * 100.) as f32;
        let runs_out = (self.used > 0.)
            .then(|| {
                let rate = f64::from(self.used) / elapsed.as_secs_f64();
                Duration::from_secs_f64(f64::from(100. - self.used) / rate)
            })
            .filter(|full| *full < left);
        Some(Pace { expected, runs_out })
    }
}

impl Pace {
    /// `11% in reserve · Lasts until reset`, as CodexBar puts it.
    pub fn describe(&self, used: f32) -> String {
        let margin = self.expected - used;
        let standing = if margin >= 0.5 {
            format!("{}% in reserve", margin.round())
        } else if margin <= -0.5 {
            format!("{}% in deficit", (-margin).round())
        } else {
            "On pace".into()
        };
        let outlook = self.runs_out.map_or_else(
            || "Lasts until reset".into(),
            |left| format!("Runs out in {}", countdown(left)),
        );
        format!("{standing} · {outlook}")
    }
}

/// How alarming a window is, from the share of it already used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

impl From<f32> for Severity {
    fn from(used: f32) -> Self {
        if used >= 80. {
            Self::Critical
        } else if used >= 60. {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

impl Severity {
    /// The word a remote client keys its gauge color on.
    pub fn token(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// The windows a compact gauge row shows: the first, which is the session
/// when there is one, and the tightest of the rest.
pub fn shown(windows: &[Window]) -> Vec<&Window> {
    let mut rest = windows.iter();
    let mut shown: Vec<&Window> = rest.next().into_iter().collect();
    shown.extend(rest.max_by(|a, b| a.used.total_cmp(&b.used)));
    shown
}

/// Whether one of the windows has nothing left, which marks its agents limited.
pub fn used_up(windows: &[Window]) -> bool {
    windows.iter().any(|window| window.left() == 0)
}

/// `47m`, `2h 53m`, `4d 11h`: the coarsest two units that still say when.
pub fn countdown(left: Duration) -> String {
    let minutes = left.as_secs().div_ceil(60);
    let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}
