#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe only runs on Windows.");
}

#[cfg(windows)]
fn main() {
    use std::{env, ffi::OsString, os::windows::ffi::OsStringExt};

    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentPackageFullName(
            package_full_name_length: *mut u32,
            package_full_name: *mut u16,
        ) -> u32;
    }

    let mut length = 0;
    let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    let result = if status == ERROR_INSUFFICIENT_BUFFER && length > 1 {
        let mut buffer = vec![0_u16; length as usize];
        let status = unsafe { GetCurrentPackageFullName(&mut length, buffer.as_mut_ptr()) };
        if status == 0 {
            let full_name = OsString::from_wide(&buffer[..length.saturating_sub(1) as usize]);
            format!("PASS {}", full_name.to_string_lossy())
        } else {
            format!("FAIL GetCurrentPackageFullName returned {status}")
        }
    } else {
        format!("FAIL package identity lookup returned {status}")
    };

    let marker_name = result
        .strip_prefix("PASS ")
        .map(|package_full_name| format!("island-phase0-msix-{package_full_name}.txt"))
        .unwrap_or_else(|| "island-phase0-msix-failed.txt".to_owned());
    let marker = env::temp_dir().join(marker_name);
    if let Err(error) = std::fs::write(&marker, &result) {
        eprintln!(
            "Could not write identity result to {}: {error}",
            marker.display()
        );
        std::process::exit(1);
    }
}
