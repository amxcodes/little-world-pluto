#[path = "../app/platform/windows/privacy.rs"]
mod privacy;

fn main() {
    match privacy::read_current_activity() {
        Ok(activity) => println!(
            "mic_active={} camera_active={} mic_apps={} camera_apps={}",
            activity.microphone_active(),
            activity.camera_active(),
            activity.microphone_apps.len(),
            activity.camera_apps.len(),
        ),
        Err(code) => println!("read_error={code}"),
    }
}
