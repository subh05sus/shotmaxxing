//! Functions looked up by name when first needed rather than linked: private ones a macOS may one day drop. When
//! one is missing, the feature that wanted it does without, and nothing fails to launch.

use std::ffi::{CStr, c_char, c_void};

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// `RTLD_DEFAULT`: look in every image already loaded.
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

/// The address of the first of `names` found among the images already loaded.
pub(super) fn find(names: &[&CStr]) -> Option<usize> {
    names.iter().find_map(|name| {
        // SAFETY: a lookup by name among the images already loaded.
        let address = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
        (!address.is_null()).then_some(address as usize)
    })
}
