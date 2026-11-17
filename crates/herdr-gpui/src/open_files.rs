//! macOS gives apps a soft open-file limit of 256, which a few windows on
//! several devices exceed.

#[cfg(unix)]
const CEILING: u64 = 1 << 20;

/// Raises the soft open-file limit as far as the hard limit and kernel allow.
/// macOS refuses values above `kern.maxfilesperproc`, so the value is halved
/// until accepted.
#[cfg(unix)]
pub(crate) fn raise() -> Option<u64> {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    let limit = getrlimit(Resource::Nofile);
    let current = limit.current.unwrap_or(u64::MAX);
    let mut wanted = limit.maximum.unwrap_or(u64::MAX).min(CEILING);
    while wanted > current {
        let next = Rlimit {
            current: Some(wanted),
            maximum: limit.maximum,
        };
        if setrlimit(Resource::Nofile, next).is_ok() {
            return Some(wanted);
        }
        wanted /= 2;
    }
    None
}

#[cfg(not(unix))]
pub(crate) fn raise() -> Option<u64> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use rustix::process::{Resource, getrlimit};

    #[test]
    fn raises_the_soft_limit_to_at_least_what_it_was() {
        let before = getrlimit(Resource::Nofile).current.unwrap_or(u64::MAX);
        super::raise();
        let after = getrlimit(Resource::Nofile).current.unwrap_or(u64::MAX);
        assert!(after >= before);
        assert!(after >= 4096.min(getrlimit(Resource::Nofile).maximum.unwrap_or(u64::MAX)));
    }
}
