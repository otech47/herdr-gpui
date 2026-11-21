use crate::{
    Result,
    usage::{Output, Shell},
};
use std::time::Duration;
#[cfg(unix)]
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, Weak},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MonitorKind {
    SystemLoad,
    ListeningPorts,
    Monitoring,
    AgentModels,
}

#[cfg(unix)]
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    kind: MonitorKind,
    target: String,
}

#[cfg(unix)]
#[derive(Clone)]
enum Slot {
    Connecting,
    Ready(Weak<Mutex<Shell>>),
}

#[cfg(unix)]
type Shared = Arc<Mutex<Shell>>;
#[cfg(unix)]
type Pool = (Mutex<HashMap<Key, Slot>>, Condvar);

#[cfg(unix)]
static POOL: OnceLock<Pool> = OnceLock::new();

#[cfg(unix)]
fn pool() -> &'static Pool {
    POOL.get_or_init(Default::default)
}

#[cfg(unix)]
#[derive(Clone)]
pub(crate) struct SharedShell {
    key: Key,
    shell: Shared,
}

#[cfg(unix)]
impl SharedShell {
    pub fn connect(target: &str, kind: MonitorKind) -> Result<Self> {
        Self::connect_with(target, kind, || Shell::connect(target))
    }

    fn connect_with(
        target: &str,
        kind: MonitorKind,
        connect: impl FnOnce() -> Result<Shell>,
    ) -> Result<Self> {
        let key = Key {
            kind,
            target: target.to_owned(),
        };
        let (slots, changed) = pool();
        let mut slots = slots.lock().unwrap_or_else(PoisonError::into_inner);
        let shell = loop {
            match slots.get(&key).cloned() {
                Some(Slot::Ready(shell)) => match shell.upgrade() {
                    Some(shell) => break shell,
                    None => {
                        slots.remove(&key);
                    }
                },
                Some(Slot::Connecting) => {
                    slots = changed.wait(slots).unwrap_or_else(PoisonError::into_inner);
                }
                None => {
                    slots.insert(key.clone(), Slot::Connecting);
                    drop(slots);
                    let created = connect();
                    let mut slots = pool().0.lock().unwrap_or_else(PoisonError::into_inner);
                    match created {
                        Ok(created) => {
                            let shell = Arc::new(Mutex::new(created));
                            slots.insert(key.clone(), Slot::Ready(Arc::downgrade(&shell)));
                            changed.notify_all();
                            break shell;
                        }
                        Err(error) => {
                            slots.remove(&key);
                            changed.notify_all();
                            return Err(error);
                        }
                    }
                }
            }
        };
        Ok(Self { key, shell })
    }

    pub fn run(&self, step: &str, timeout: Duration) -> Result<Output> {
        let result = self
            .shell
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .run(step, timeout);
        if result.is_err() {
            let mut pool = pool().0.lock().unwrap_or_else(PoisonError::into_inner);
            let same = match pool.get(&self.key) {
                Some(Slot::Ready(shell)) => shell
                    .upgrade()
                    .is_some_and(|shell| Arc::ptr_eq(&shell, &self.shell)),
                _ => false,
            };
            if same {
                pool.remove(&self.key);
            }
        }
        result
    }
}

pub(crate) enum MonitorShell {
    Local(Shell),
    #[cfg(unix)]
    Remote(SharedShell),
}

impl MonitorShell {
    pub fn local(shell: Shell) -> Self {
        Self::Local(shell)
    }

    #[cfg(unix)]
    pub fn remote(target: &str, kind: MonitorKind) -> Result<Self> {
        SharedShell::connect(target, kind).map(Self::Remote)
    }

    #[cfg(windows)]
    pub fn remote(_target: &str, _kind: MonitorKind) -> Result<Self> {
        Err(crate::Error::UsageUnsupported)
    }

    pub fn run(&mut self, step: &str, timeout: Duration) -> Result<Output> {
        match self {
            Self::Local(shell) => shell.run(step, timeout),
            #[cfg(unix)]
            Self::Remote(shell) => shell.run(step, timeout),
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
