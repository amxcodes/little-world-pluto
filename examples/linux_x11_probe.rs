#[cfg(not(target_os = "linux"))]
compile_error!("linux_x11_probe only builds for Linux");

use std::{env, error::Error};

use x11rb::{
    connection::{Connection, RequestConnection},
    protocol::xproto::{Atom, AtomEnum, ConnectionExt as XprotoConnectionExt},
    rust_connection::RustConnection,
};

fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom, Box<dyn Error>> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn main() -> Result<(), Box<dyn Error>> {
    let (conn, screen_index) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_index];

    println!(
        "DISPLAY={}",
        env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    println!(
        "WAYLAND_DISPLAY={}",
        env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "<unset>".into())
    );
    println!(
        "X11 vendor: {}",
        String::from_utf8_lossy(&conn.setup().vendor)
    );
    println!(
        "X11 protocol: {}.{}",
        conn.setup().protocol_major_version,
        conn.setup().protocol_minor_version
    );
    println!(
        "Root screen: {}x{}",
        screen.width_in_pixels, screen.height_in_pixels
    );

    for name in ["SHAPE", "XFIXES", "RANDR", "Composite", "RENDER"] {
        let present = conn.extension_information(name)?.is_some();
        println!(
            "Extension {name}: {}",
            if present { "present" } else { "absent" }
        );
    }

    let wm_check_atom = intern_atom(&conn, "_NET_SUPPORTING_WM_CHECK")?;
    let wm_check = conn
        .get_property(false, screen.root, wm_check_atom, AtomEnum::WINDOW, 0, 1)?
        .reply()?;
    let wm_window = wm_check.value32().and_then(|mut words| words.next());
    match wm_window {
        Some(window) => println!("_NET_SUPPORTING_WM_CHECK: window {window:#x}"),
        None => println!("_NET_SUPPORTING_WM_CHECK: absent or unreadable"),
    }

    let supported_atom = intern_atom(&conn, "_NET_SUPPORTED")?;
    let supported_reply = conn
        .get_property(false, screen.root, supported_atom, AtomEnum::ATOM, 0, 1024)?
        .reply()?;
    let supported_atoms = supported_reply
        .value32()
        .map(|words| words.collect::<Vec<_>>());

    println!("EWMH root support:");
    for name in [
        "_NET_WM_WINDOW_TYPE_DOCK",
        "_NET_WM_STATE_ABOVE",
        "_NET_WM_STATE_SKIP_TASKBAR",
        "_NET_WM_STATE_SKIP_PAGER",
    ] {
        let state = match supported_atoms.as_ref() {
            Some(atoms) => {
                if atoms.contains(&intern_atom(&conn, name)?) {
                    "advertised"
                } else {
                    "not advertised"
                }
            }
            None => "unknown",
        };
        println!("  {name}: {state}");
    }

    println!("Environment probe only: no window was created or mapped.");
    Ok(())
}
