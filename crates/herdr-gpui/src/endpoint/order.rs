//! Code across the app expects this computer at endpoint index 0, so the
//! listing order is a permutation of the endpoint indices.

use super::{LOCAL, saved_profile_id};
use crate::HerdrWindow;

/// Devices `order` names, by label, id or saved profile id, lead in its order.
pub(crate) fn ordered<'a>(
    order: &[String],
    devices: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<usize> {
    let named = |id: &str, label: &str| {
        order.iter().position(|name| {
            let name = name.trim();
            name.eq_ignore_ascii_case(label)
                || name == id
                || saved_profile_id(id) == Some(name)
                || (id == LOCAL && name.eq_ignore_ascii_case(LOCAL))
        })
    };
    let mut ranked: Vec<_> = devices
        .into_iter()
        .enumerate()
        .map(|(index, (id, label))| (named(id, label).unwrap_or(usize::MAX), index))
        .collect();
    ranked.sort();
    ranked.into_iter().map(|(_, index)| index).collect()
}

impl HerdrWindow {
    pub(crate) fn device_order(&self) -> Vec<usize> {
        ordered(
            &self.config.device_order,
            self.endpoints
                .iter()
                .map(|endpoint| (endpoint.id.as_str(), endpoint.label.as_str())),
        )
    }
}

#[cfg(test)]
mod tests;
