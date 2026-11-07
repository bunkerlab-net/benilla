// SPDX-License-Identifier: GPL-3.0-or-later
#[cfg(benilla_native_generator)]
use std::io::Read;

#[cfg(benilla_native_generator)]
unsafe extern "C" {
    fn benilla_nav_generate(input: *const u8, len: usize) -> i32;
}

#[cfg(benilla_native_generator)]
fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("native-navigation: arguments are not supported; use the bounded stdin protocol");
        std::process::exit(2);
    }
    let limit = benilla_native_nav_generator::MAX_REQUEST_BYTES;
    let mut bytes = Vec::new();
    if let Err(error) = std::io::stdin()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
    {
        eprintln!("native-navigation: stdin read failed: {error}");
        std::process::exit(2);
    }
    if bytes.len() > limit {
        eprintln!("native-navigation: request exceeds byte limit");
        std::process::exit(2);
    }
    // The sole native entry point lives in this executable; all C++ exceptions are caught there.
    let result = unsafe { benilla_nav_generate(bytes.as_ptr(), bytes.len()) };
    if result != 0 {
        std::process::exit(result);
    }
}

#[cfg(not(benilla_native_generator))]
fn main() {
    eprintln!("native-navigation: native generation is unsupported on this target; the protocol library remains available");
    std::process::exit(3);
}
