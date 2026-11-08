use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HANDLE, HWND};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

type WaitForClock = unsafe extern "system" fn(u32, *const HANDLE, u32) -> u32;

pub(super) fn start(window: HWND, message: u32) -> std::io::Result<SyncSender<Duration>> {
    let (send, receive) = sync_channel::<Duration>(1);
    let window = window as usize;
    std::thread::Builder::new()
        .name("pluto-figure-clock".into())
        .spawn(move || {
            let name: Vec<_> = "dcomp.dll".encode_utf16().chain([0]).collect();
            // DirectComposition is already loaded by the native figure renderer.
            let module = unsafe { GetModuleHandleW(name.as_ptr()) };
            let mut wait: Option<WaitForClock> = if module.is_null() {
                None
            } else {
                unsafe {
                    GetProcAddress(
                        module,
                        c"DCompositionWaitForCompositorClock".as_ptr().cast(),
                    )
                }
                .map(|address| unsafe { std::mem::transmute(address) })
            };
            let mut deadline = None;
            while let Ok(refresh_period) = receive.recv() {
                let interval = refresh_period.max(Duration::from_secs_f64(1.0 / 60.0));
                loop {
                    let paced = if let Some(clock) = wait {
                        if unsafe { clock(0, std::ptr::null(), 50) == 0 } {
                            true
                        } else {
                            wait = None;
                            unsafe { DwmFlush() >= 0 }
                        }
                    } else {
                        unsafe { DwmFlush() >= 0 }
                    };
                    if !paced {
                        std::thread::sleep(interval);
                    }
                    let now = Instant::now();
                    // Allow for compositor timestamp jitter at nominal 60 Hz
                    // while preserving the long-term deadline on faster panels.
                    if deadline.is_none_or(|due| now + Duration::from_millis(1) >= due) {
                        deadline = Some(next_deadline(deadline, now, interval));
                        break;
                    }
                }
                if unsafe { PostMessageW(window as HWND, message, 0, 0) } == 0 {
                    break;
                }
            }
        })?;
    Ok(send)
}

fn next_deadline(previous: Option<Instant>, now: Instant, interval: Duration) -> Instant {
    match previous {
        Some(previous) if now.saturating_duration_since(previous) < interval => previous + interval,
        _ => now + interval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_keeps_cadence_without_catching_up_after_idle() {
        let origin = Instant::now();
        let interval = Duration::from_millis(17);
        let due = origin + interval;
        assert_eq!(
            next_deadline(Some(due), origin + Duration::from_millis(16), interval),
            due + interval
        );
        let resumed = origin + Duration::from_secs(1);
        assert_eq!(
            next_deadline(Some(due), resumed, interval),
            resumed + interval
        );
    }
}
