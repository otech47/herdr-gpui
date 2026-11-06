//! File promises, which the screenshot thumbnail, browsers and Photos drag
//! instead of paths. GPUI's window takes paths only, so its drag callbacks are
//! wrapped here.
//!
//! This needs `unsafe`: GPUI defines the window class and its callbacks with no
//! hook, so the only ways in are replacing those methods or patching the
//! pinned GPUI. The bookkeeping is safe code.
#![allow(unsafe_code)]

use crate::{Error, Result};
use block2::RcBlock;
use gpui::{Pixels, Point, Window, WindowId, point, px};
use objc2::{
    ClassType,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSDragOperation, NSDraggingInfo, NSFilePromiseReceiver, NSPasteboard, NSView, NSWindow,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue, NSString, NSURL};
use std::{
    path::PathBuf,
    ptr::NonNull,
    sync::{
        Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

const MAX_PENDING: usize = 16;

type DragHook = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize;
type PerformHook = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool;

struct Originals {
    entered: DragHook,
    updated: DragHook,
    perform: PerformHook,
}

static ORIGINALS: OnceLock<Option<Originals>> = OnceLock::new();
static WINDOWS: Mutex<Vec<(usize, WindowId)>> = Mutex::new(Vec::new());
static PENDING: Mutex<Pending> = Mutex::new(Pending { drops: Vec::new() });
static NEXT_DROP: AtomicU64 = AtomicU64::new(0);

thread_local! {
    // The reader waits on file coordination, which must stay off the main queue.
    static QUEUE: Retained<NSOperationQueue> = NSOperationQueue::new();
}

pub(in crate::window) struct Promised {
    pub(in crate::window) position: Point<Pixels>,
    pub(in crate::window) paths: Vec<PathBuf>,
    pub(in crate::window) errors: Vec<Error>,
}

struct Entry {
    id: u64,
    window: WindowId,
    expected: usize,
    drop: Promised,
}

struct Pending {
    drops: Vec<Entry>,
}

impl Pending {
    fn begin(&mut self, id: u64, window: WindowId, position: Point<Pixels>, expected: usize) {
        if self.drops.len() >= MAX_PENDING {
            self.drops.remove(0);
        }
        self.drops.push(Entry {
            id,
            window,
            expected: expected.max(1),
            drop: Promised {
                position,
                paths: Vec::new(),
                errors: Vec::new(),
            },
        });
    }

    fn arrived(&mut self, id: u64, file: Result<PathBuf>) {
        let Some(entry) = self.drops.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        match file {
            Ok(path) => entry.drop.paths.push(path),
            Err(error) => entry.drop.errors.push(error),
        }
    }

    fn take(&mut self, window: WindowId) -> Vec<Promised> {
        let (done, waiting) = std::mem::take(&mut self.drops)
            .into_iter()
            .partition::<Vec<_>, _>(|entry| {
                entry.window == window
                    && entry.drop.paths.len() + entry.drop.errors.len() >= entry.expected
            });
        self.drops = waiting;
        done.into_iter().map(|entry| entry.drop).collect()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(in crate::window) fn take(window: WindowId) -> Vec<Promised> {
    lock(&PENDING).take(window)
}

/// Lets `window` take file promises as well as paths.
pub(in crate::window) fn accept(window: &Window) -> Result<()> {
    let handle = HasWindowHandle::window_handle(window)?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Err(Error::FilePromiseWindow);
    };
    // SAFETY: an AppKit handle's view is GPUI's live content view while the
    // window is borrowed, and window construction runs on the main thread.
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let native = view.window().ok_or(Error::FilePromiseWindow)?;
    ORIGINALS
        .get_or_init(install)
        .as_ref()
        .ok_or(Error::FilePromiseWindow)?;
    // The legacy name GPUI registers, so its own path drops keep working.
    let mut types = vec![NSString::from_str("NSFilenamesPboardType")];
    types.extend(NSFilePromiseReceiver::readableDraggedTypes());
    native.registerForDraggedTypes(&NSArray::from_retained_slice(&types));
    let key = Retained::as_ptr(&native) as usize;
    let id = window.window_handle().window_id();
    let mut windows = lock(&WINDOWS);
    windows.retain(|(native, window)| *native != key && *window != id);
    windows.push((key, id));
    Ok(())
}

fn install() -> Option<Originals> {
    let class = AnyClass::get(c"GPUIWindow")?;
    // Only GPUIWindow's own methods: replacing an inherited one would change
    // every NSWindow in the process.
    let methods = class.instance_methods();
    let find = |name: Sel| methods.iter().find(|method| method.name() == name).copied();
    let entered = find(sel!(draggingEntered:))?;
    let updated = find(sel!(draggingUpdated:))?;
    let perform = find(sel!(performDragOperation:))?;
    // SAFETY: the pinned GPUI defines these on GPUIWindow with the ABI of
    // DragHook and PerformHook, and each original is called back only through
    // its own type. This runs on the main thread, where AppKit calls them.
    unsafe {
        Some(Originals {
            entered: std::mem::transmute::<Imp, DragHook>(
                entered.set_implementation(std::mem::transmute::<DragHook, Imp>(dragging_entered)),
            ),
            updated: std::mem::transmute::<Imp, DragHook>(
                updated.set_implementation(std::mem::transmute::<DragHook, Imp>(dragging_updated)),
            ),
            perform: std::mem::transmute::<Imp, PerformHook>(perform.set_implementation(
                std::mem::transmute::<PerformHook, Imp>(perform_drag_operation),
            )),
        })
    }
}

fn originals() -> Option<&'static Originals> {
    ORIGINALS.get().and_then(Option::as_ref)
}

fn window_id(native: *mut AnyObject) -> Option<WindowId> {
    let key = native as usize;
    lock(&WINDOWS)
        .iter()
        .find(|(native, _)| *native == key)
        .map(|(_, id)| *id)
}

/// A promise this window takes. A drag that also carries paths stays GPUI's.
///
/// # Safety
///
/// `info` must be the dragging info AppKit passed to the current callback.
unsafe fn promise<'a>(
    native: *mut AnyObject,
    info: *mut AnyObject,
) -> Option<(WindowId, &'a ProtocolObject<dyn NSDraggingInfo>)> {
    let window = window_id(native)?;
    // SAFETY: AppKit passes an object conforming to NSDraggingInfo that lives
    // for the duration of the callback.
    let info = unsafe { info.cast::<ProtocolObject<dyn NSDraggingInfo>>().as_ref() }?;
    let pasteboard = info.draggingPasteboard();
    let paths = NSArray::from_retained_slice(&[NSString::from_str("NSFilenamesPboardType")]);
    (pasteboard.availableTypeFromArray(&paths).is_none()
        && pasteboard
            .availableTypeFromArray(&NSFilePromiseReceiver::readableDraggedTypes())
            .is_some())
    .then_some((window, info))
}

unsafe extern "C-unwind" fn dragging_entered(
    native: *mut AnyObject,
    selector: Sel,
    info: *mut AnyObject,
) -> usize {
    // SAFETY: AppKit calls this with its own dragging info.
    if unsafe { promise(native, info) }.is_some() {
        return NSDragOperation::Copy.0;
    }
    // SAFETY: the arguments are the ones AppKit passed to this selector.
    originals().map_or(NSDragOperation::None.0, |originals| unsafe {
        (originals.entered)(native, selector, info)
    })
}

unsafe extern "C-unwind" fn dragging_updated(
    native: *mut AnyObject,
    selector: Sel,
    info: *mut AnyObject,
) -> usize {
    // SAFETY: AppKit calls this with its own dragging info.
    if unsafe { promise(native, info) }.is_some() {
        return NSDragOperation::Copy.0;
    }
    // SAFETY: the arguments are the ones AppKit passed to this selector.
    originals().map_or(NSDragOperation::None.0, |originals| unsafe {
        (originals.updated)(native, selector, info)
    })
}

unsafe extern "C-unwind" fn perform_drag_operation(
    native: *mut AnyObject,
    selector: Sel,
    info: *mut AnyObject,
) -> Bool {
    // SAFETY: AppKit calls this with its own dragging info.
    if let Some((window, info)) = unsafe { promise(native, info) } {
        // SAFETY: AppKit sends this selector to the GPUIWindow, an NSWindow.
        let height = unsafe { native.cast::<NSWindow>().as_ref() }
            .and_then(NSWindow::contentView)
            .map_or(0., |view| view.frame().size.height);
        let location = info.draggingLocation();
        let position = point(px(location.x as f32), px((height - location.y) as f32));
        return match receive(window, position, &info.draggingPasteboard()) {
            Ok(()) => Bool::YES,
            Err(error) => {
                tracing::warn!(%error, "Could not receive the dragged files");
                Bool::NO
            }
        };
    }
    // SAFETY: the arguments are the ones AppKit passed to this selector.
    originals().map_or(Bool::NO, |originals| unsafe {
        (originals.perform)(native, selector, info)
    })
}

fn receive(window: WindowId, position: Point<Pixels>, pasteboard: &NSPasteboard) -> Result<()> {
    let classes = NSArray::from_slice(&[NSFilePromiseReceiver::class()]);
    // SAFETY: NSFilePromiseReceiver implements NSPasteboardReading, and no
    // reading options are passed.
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, None) };
    let receivers: Vec<_> = objects
        .into_iter()
        .flatten()
        .filter_map(|object| object.downcast::<NSFilePromiseReceiver>().ok())
        .collect();
    if receivers.is_empty() {
        return Err(Error::FilePromise {
            reason: "the drag carried no readable promise".into(),
        });
    }
    let id = NEXT_DROP.fetch_add(1, Ordering::Relaxed);
    // One folder per drop: two drops of the same screenshot carry the same name.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let folder = std::env::temp_dir()
        .join("herdr-drops")
        .join(format!("{}-{stamp}-{id}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|source| Error::FilePromiseFolder {
        path: folder.clone(),
        source,
    })?;
    let destination = NSURL::from_directory_path(&folder).ok_or(Error::FilePromiseWindow)?;
    let expected = receivers
        .iter()
        .map(|receiver| receiver.fileTypes().count().max(1))
        .sum();
    lock(&PENDING).begin(id, window, position, expected);
    let reader = RcBlock::new(move |file: NonNull<NSURL>, error: *mut NSError| {
        // SAFETY: AppKit passes a valid URL, and an error only when writing failed.
        let result = match unsafe { error.as_ref() } {
            Some(error) => Err(Error::FilePromise {
                reason: error.localizedDescription().to_string(),
            }),
            None => unsafe { file.as_ref() }
                .to_file_path()
                .ok_or_else(|| Error::FilePromise {
                    reason: "the file has no local path".into(),
                }),
        };
        lock(&PENDING).arrived(id, result);
    });
    let options = NSDictionary::new();
    QUEUE.with(|queue| {
        for receiver in &receivers {
            // SAFETY: the options dictionary is empty and the queue is a private
            // background queue that outlives the receiver's work.
            unsafe {
                receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                    &destination,
                    &options,
                    queue,
                    &reader,
                );
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
