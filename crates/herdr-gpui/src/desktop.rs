//! The macOS desktop each window shows. AppKit has no call for it, so
//! SkyLight's are looked up on first use, and without them nothing moves.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Desktop {
    /// Kept across logins, unlike `id`. macOS gives some desktops none.
    pub uuid: String,
    pub id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Done,
    /// The desktop no longer exists.
    Missing,
    /// The window is not on screen yet, or did not move.
    Retry,
}

#[cfg(any(target_os = "macos", test))]
fn find(saved: &Desktop, desktops: &[Desktop]) -> Option<u64> {
    desktops
        .iter()
        .find(|desktop| {
            if saved.uuid.is_empty() {
                desktop.id == saved.id
            } else {
                desktop.uuid == saved.uuid
            }
        })
        .map(|desktop| desktop.id)
}

#[cfg(target_os = "macos")]
pub(crate) use native::{move_to, of};

#[cfg(not(target_os = "macos"))]
pub(crate) fn of(_: &gpui::Window) -> Option<Desktop> {
    None
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn move_to(_: &gpui::Window, _: &Desktop) -> Move {
    Move::Missing
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native {
    use super::{Desktop, Move, find};
    use gpui::Window;
    use objc2_app_kit::NSView;
    use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};
    use std::{
        ffi::{CStr, c_char, c_int, c_void},
        ptr::NonNull,
        sync::OnceLock,
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const SKYLIGHT: &CStr = c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight";
    const RTLD_LAZY: c_int = 1;
    /// User, fullscreen and system desktops alike.
    const ALL_DESKTOPS: c_int = 7;

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(library: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    struct SkyLight {
        connection: unsafe extern "C" fn() -> c_int,
        desktops_of_windows: unsafe extern "C" fn(c_int, c_int, *const c_void) -> *mut c_void,
        desktops: unsafe extern "C" fn(c_int) -> *mut c_void,
        move_windows: unsafe extern "C" fn(c_int, *const c_void, u64),
    }

    fn skylight() -> Option<&'static SkyLight> {
        static LOADED: OnceLock<Option<SkyLight>> = OnceLock::new();
        LOADED
            .get_or_init(|| {
                // SAFETY: both calls take NUL-terminated names and return
                // null on failure, checked before a pointer is used. Each
                // symbol has the signature its field declares.
                unsafe {
                    let library = NonNull::new(dlopen(SKYLIGHT.as_ptr(), RTLD_LAZY))?;
                    let symbol = |name: &CStr| NonNull::new(dlsym(library.as_ptr(), name.as_ptr()));
                    Some(SkyLight {
                        connection: std::mem::transmute::<
                            NonNull<c_void>,
                            unsafe extern "C" fn() -> c_int,
                        >(symbol(c"SLSMainConnectionID")?),
                        desktops_of_windows: std::mem::transmute::<
                            NonNull<c_void>,
                            unsafe extern "C" fn(c_int, c_int, *const c_void) -> *mut c_void,
                        >(symbol(
                            c"SLSCopySpacesForWindows",
                        )?),
                        desktops: std::mem::transmute::<
                            NonNull<c_void>,
                            unsafe extern "C" fn(c_int) -> *mut c_void,
                        >(symbol(
                            c"SLSCopyManagedDisplaySpaces",
                        )?),
                        move_windows: std::mem::transmute::<
                            NonNull<c_void>,
                            unsafe extern "C" fn(c_int, *const c_void, u64),
                        >(symbol(
                            c"SLSMoveWindowsToManagedSpace",
                        )?),
                    })
                }
            })
            .as_ref()
    }

    fn window_number(window: &Window) -> Option<isize> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // SAFETY: an AppKit handle's view is GPUI's live content view while
        // the window is borrowed, on the main thread.
        let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        Some(view.window()?.windowNumber())
    }

    fn array(object: CFRetained<CFType>) -> Option<CFRetained<CFArray<CFType>>> {
        let array = object.downcast::<CFArray>().ok()?;
        // SAFETY: SkyLight's arrays hold CF objects.
        Some(unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) })
    }

    fn dictionary(
        object: CFRetained<CFType>,
    ) -> Option<CFRetained<CFDictionary<CFString, CFType>>> {
        let dictionary = object.downcast::<CFDictionary>().ok()?;
        // SAFETY: SkyLight's dictionaries map strings to CF objects.
        Some(unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(dictionary) })
    }

    /// Takes ownership of what a SkyLight `Copy` call returned.
    fn copied(result: *mut c_void) -> Option<CFRetained<CFArray<CFType>>> {
        let result = NonNull::new(result.cast::<CFType>())?;
        // SAFETY: a `Copy` call returns an owned reference or null.
        array(unsafe { CFRetained::from_raw(result) })
    }

    fn number(object: CFRetained<CFType>) -> Option<u64> {
        u64::try_from(object.downcast::<CFNumber>().ok()?.as_i64()?).ok()
    }

    fn one_window(number: isize) -> CFRetained<CFArray<CFNumber>> {
        CFArray::from_retained_objects(&[CFNumber::new_isize(number)])
    }

    fn desktops_of(skylight: &SkyLight, connection: c_int, window: isize) -> Vec<u64> {
        let windows = one_window(window);
        let windows: *const CFArray<CFNumber> = &*windows;
        // SAFETY: the array outlives the call.
        let result =
            unsafe { (skylight.desktops_of_windows)(connection, ALL_DESKTOPS, windows.cast()) };
        copied(result).map_or_else(Vec::new, |desktops| {
            desktops.iter().filter_map(number).collect()
        })
    }

    fn desktops(skylight: &SkyLight, connection: c_int) -> Vec<Desktop> {
        // SAFETY: takes the connection only.
        let Some(displays) = copied(unsafe { (skylight.desktops)(connection) }) else {
            return Vec::new();
        };
        let spaces = CFString::from_static_str("Spaces");
        let id = CFString::from_static_str("ManagedSpaceID");
        let uuid = CFString::from_static_str("uuid");
        displays
            .iter()
            .filter_map(|display| array(dictionary(display)?.get(&spaces)?))
            .flat_map(|desktops| desktops.iter().collect::<Vec<_>>())
            .filter_map(|desktop| {
                let desktop = dictionary(desktop)?;
                Some(Desktop {
                    id: number(desktop.get(&id)?)?,
                    uuid: desktop
                        .get(&uuid)
                        .and_then(|uuid| uuid.downcast::<CFString>().ok())
                        .map(|uuid| uuid.to_string())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    /// The one desktop `window` shows, or none when it shows on every desktop
    /// or is not on screen.
    pub(crate) fn of(window: &Window) -> Option<Desktop> {
        let skylight = skylight()?;
        let number = window_number(window)?;
        // SAFETY: takes no arguments.
        let connection = unsafe { (skylight.connection)() };
        let [id] = desktops_of(skylight, connection, number)[..] else {
            return None;
        };
        desktops(skylight, connection)
            .into_iter()
            .find(|desktop| desktop.id == id)
    }

    /// Moves `window`, which this process owns, to `saved`.
    pub(crate) fn move_to(window: &Window, saved: &Desktop) -> Move {
        let (Some(skylight), Some(number)) = (skylight(), window_number(window)) else {
            return Move::Missing;
        };
        // SAFETY: takes no arguments.
        let connection = unsafe { (skylight.connection)() };
        let Some(target) = find(saved, &desktops(skylight, connection)) else {
            return Move::Missing;
        };
        let current = desktops_of(skylight, connection, number);
        if current.is_empty() {
            return Move::Retry;
        }
        if current != [target] {
            let windows = one_window(number);
            let windows: *const CFArray<CFNumber> = &*windows;
            // SAFETY: the array outlives the call.
            unsafe { (skylight.move_windows)(connection, windows.cast(), target) };
        }
        if desktops_of(skylight, connection, number) == [target] {
            Move::Done
        } else {
            Move::Retry
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop(uuid: &str, id: u64) -> Desktop {
        Desktop {
            uuid: uuid.into(),
            id,
        }
    }

    #[test]
    fn a_saved_desktop_is_found_by_uuid_after_its_id_changes() {
        let now = [desktop("", 1), desktop("AA", 41), desktop("BB", 57)];
        assert_eq!(find(&desktop("BB", 9), &now), Some(57));
        assert_eq!(find(&desktop("CC", 41), &now), None);
        assert_eq!(find(&desktop("", 1), &now), Some(1));
        assert_eq!(find(&desktop("", 2), &now), None);
    }
}
