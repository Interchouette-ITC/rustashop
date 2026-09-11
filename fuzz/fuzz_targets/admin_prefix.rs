#![no_main]

use libfuzzer_sys::fuzz_target;
use rustashop_api::AdminApiPrefix;

fuzz_target!(|data: &[u8]| {
    let Ok(raw) = std::str::from_utf8(data) else {
        return;
    };
    let _ = AdminApiPrefix::parse(raw);
});
