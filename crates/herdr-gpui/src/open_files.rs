//! The open-file limit. macOS starts an app with a soft limit of 256, and
//! every window keeps a connection, helper shells and their pipes per device,
//! which passes 256 with a few windows and devices.

/// The highest soft limit tried; well above what a session of windows uses.
#[cfg(unix)]
const CEILING: u64 = 1 << 20;

/// Raises the soft open-file limit as far as the hard limit and the kernel
/// allow. macOS refuses a soft limit above `kern.maxfilesperproc` even under
/// an unlimited hard limit, so each refused value is halved and tried again.
/// Child processes, the local daemon among them, inherit the raised limit.
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
