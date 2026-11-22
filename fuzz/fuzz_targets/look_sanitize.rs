//! The SVG of a custom look comes with someone else's project: whatever its text, the check
//! returns the rebuilt SVG or an error, and never panics.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let _ = whiteboxed::look::sanitize(text);
    }
});
