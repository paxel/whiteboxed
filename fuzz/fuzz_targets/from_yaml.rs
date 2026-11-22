//! A project file comes from anyone's repository: whatever its text, loading it returns a
//! project or an error, and never panics.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(text) = std::str::from_utf8(bytes) {
        let _ = whiteboxed::persist::from_yaml(text);
    }
});
