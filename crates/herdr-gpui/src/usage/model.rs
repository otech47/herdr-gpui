//! Plan usage in the terms the status bar and its panel show: which agent,
//! which account, which limit windows, how much of each is used, when each
//! resets, and whatever else the agent's service reports.

use super::service::Service;
use herdr_client::ConnectTarget;
use std::{any::Any, sync::Arc};

pub(crate) use herdr_board::usage::{
    DAY, Kind, MONTH, Pace, SESSION, Severity, WEEK, Window, countdown,
};

/// The machine whose agent sign-ins are read. A remote host is asked over SSH,
/// so its own credentials are used and never leave it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Host {
    Local,
    Ssh(String),
}

impl From<&ConnectTarget> for Host {
    fn from(target: &ConnectTarget) -> Self {
        match target {
            ConnectTarget::Ssh { target, .. } => Self::Ssh(target.clone()),
            ConnectTarget::Local | ConnectTarget::Session { .. } | ConnectTarget::Socket(_) => {
                Self::Local
            }
        }
    }
}

/// A registered provider. Only [`super::registry`] makes one, so each value
/// is a real [`Service`]; two are equal when their ids are.
#[derive(Clone, Copy)]
pub(crate) struct Provider(pub(super) &'static dyn Service);

impl Provider {
    pub fn service(self) -> &'static dyn Service {
        self.0
    }

    pub fn id(self) -> &'static str {
        self.0.meta().id
    }

    pub fn name(self) -> &'static str {
        self.0.meta().name
    }

    pub fn icon(self) -> &'static str {
        self.0.meta().icon_path()
    }
}

impl PartialEq for Provider {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for Provider {}

impl std::hash::Hash for Provider {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Provider({})", self.id())
    }
}

/// Who is signed in, as far as the sign-in and the service say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Account {
    pub email: Option<String>,
    pub plan: Option<String>,
}

/// Anything else a service reports, in shapes the panel knows how to draw, so
/// a new provider adds detail without touching the panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Section {
    /// A further limit that is not part of the headline windows.
    Limit(Window),
    /// Label and value pairs under a heading.
    Facts {
        title: String,
        facts: Vec<(String, String)>,
    },
    /// Shares of one whole, such as which surfaces spent a weekly limit.
    Shares {
        title: String,
        shares: Vec<(String, f32)>,
    },
}

/// Money or credits left, or spent, in the unit the service counts in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Balance {
    pub label: String,
    pub amount: f64,
    pub unit: Unit,
    /// What the amount is out of, when the service says, e.g. a monthly budget.
    pub total: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// An ISO 4217 code such as `USD`.
    Currency(String),
    /// A count of the service's own units, named as it names them.
    Count(String),
}

impl Balance {
    pub fn new(label: impl Into<String>, amount: f64, unit: Unit) -> Self {
        Self {
            label: label.into(),
            amount,
            unit,
            total: None,
        }
    }

    pub fn out_of(mut self, total: f64) -> Self {
        self.total = Some(total);
        self
    }

    /// `$12.30`, `12.30 EUR`, or `1,250 credits`.
    pub fn amount_text(&self) -> String {
        amount_text(self.amount, &self.unit)
    }

    pub fn text(&self) -> String {
        match self.total {
            Some(total) => format!(
                "{} of {}",
                self.amount_text(),
                amount_text(total, &self.unit)
            ),
            None => self.amount_text(),
        }
    }
}

fn amount_text(amount: f64, unit: &Unit) -> String {
    match unit {
        Unit::Currency(code) if code == "USD" => format!("${amount:.2}"),
        Unit::Currency(code) => format!("{amount:.2} {code}"),
        Unit::Count(name) => {
            let whole = amount.round();
            let text = if (amount - whole).abs() < 0.005 {
                group(whole as i64)
            } else {
                format!("{amount:.2}")
            };
            format!("{text} {name}")
        }
    }
}

/// `1250000` as `1,250,000`.
pub(crate) fn group(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if value < 0 {
        out.push('-');
    }
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A provider's own data for its own [`Service::render`], beyond what the
/// shared fields carry. It crosses from the worker thread, so it is shared.
pub(crate) type Detail = Arc<dyn Any + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Report {
    pub provider: Provider,
    pub account: Account,
    /// Session, then weekly, then model windows.
    pub windows: Vec<Window>,
    pub balances: Vec<Balance>,
    pub sections: Vec<Section>,
    pub detail: Option<Detail>,
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Report")
            .field("provider", &self.provider)
            .field("account", &self.account)
            .field("windows", &self.windows)
            .field("balances", &self.balances)
            .field("sections", &self.sections)
            .field("detail", &self.detail.is_some())
            .finish()
    }
}

/// Provider detail is opaque, so two reports match when everything shown
/// through the shared fields does.
impl PartialEq for Report {
    fn eq(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.account == other.account
            && self.windows == other.windows
            && self.balances == other.balances
            && self.sections == other.sections
    }
}

impl Report {
    pub fn new(provider: Provider, account: Account, mut windows: Vec<Window>) -> Self {
        windows.sort_by(|a, b| a.kind.cmp(&b.kind));
        Self {
            provider,
            account,
            windows,
            balances: Vec::new(),
            sections: Vec::new(),
            detail: None,
        }
    }

    pub fn with_sections(mut self, sections: impl IntoIterator<Item = Section>) -> Self {
        self.sections.extend(sections);
        self
    }

    pub fn with_balances(mut self, balances: impl IntoIterator<Item = Balance>) -> Self {
        self.balances.extend(balances);
        self
    }

    pub fn with_detail(mut self, detail: impl Any + Send + Sync) -> Self {
        self.detail = Some(Arc::new(detail));
        self
    }

    /// The provider's own data, if it stored this type.
    pub fn detail<T: Any>(&self) -> Option<&T> {
        self.detail.as_deref()?.downcast_ref()
    }

    /// The window closest to its limit, which ranks the provider.
    pub fn tightest(&self) -> Option<&Window> {
        self.windows.iter().max_by(|a, b| a.used.total_cmp(&b.used))
    }

    /// The windows the status bar shows: the first, which is the session
    /// when there is one, and the tightest of the rest. The panel lists all.
    pub fn shown(&self) -> Vec<&Window> {
        herdr_board::usage::shown(&self.windows)
    }
}

/// `pro` to `Pro`, `max` to `Max`: plan identifiers read as names.
pub(crate) fn title_case(word: &str) -> String {
    let mut chars = word.trim().chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
