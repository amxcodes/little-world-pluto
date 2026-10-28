//! Short-lived, in-memory system playback capture for Meetings. The endpoint
//! loopback is created only after the user presses Start and is stopped with
//! the meeting. No raw audio is written to disk.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_startup::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows_startup::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

const WM_APP_SHELL_TICK: u32 = 0x8000 + 0x51;
const OUTPUT_RATE: u32 = 16_000;
const PACKET_SAMPLES: usize = 1_600;

pub enum AudioEvent {
    Ready,
    Samples(Vec<u8>),
    Failed(String),
}

pub struct MeetingLoopback {
    stop: Arc<AtomicBool>,
    events: Receiver<AudioEvent>,
    worker: Option<JoinHandle<()>>,
}

impl MeetingLoopback {
    pub fn start(window: isize) -> Self {
        let (sender, events) = sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let result = capture_playback(&worker_stop, &sender, window);
            if let Err(error) = result {
                let mut event = AudioEvent::Failed(error);
                for _ in 0..20 {
                    match sender.try_send(event) {
                        Ok(()) => {
                            unsafe {
                                PostMessageW(window as _, WM_APP_SHELL_TICK, 0, 0);
                            }
                            break;
                        }
                        Err(TrySendError::Full(pending)) => {
                            event = pending;
                            unsafe {
                                PostMessageW(window as _, WM_APP_SHELL_TICK, 0, 0);
                            }
                            if worker_stop.load(Ordering::Acquire) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
            }
        });
        Self {
            stop,
            events,
            worker: Some(worker),
        }
    }

    pub fn next(&self) -> Option<AudioEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for MeetingLoopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn notify(sender: &SyncSender<AudioEvent>, window: isize, event: AudioEvent) {
    if sender.try_send(event).is_ok() {
        unsafe {
            PostMessageW(window as _, WM_APP_SHELL_TICK, 0, 0);
        }
    }
}

#[derive(Clone, Copy)]
enum SampleFormat {
    Float32,
    Int16,
    Int24,
    Int32,
}

fn sample_format(format: *const WAVEFORMATEX) -> Result<SampleFormat, String> {
    let format_ptr = format;
    let format = unsafe { format.read_unaligned() };
    let tag = format.wFormatTag;
    let bits = format.wBitsPerSample;
    let subtype = if tag == 0xfffe && format.cbSize >= 22 {
        let extensible = unsafe { (format_ptr as *const WAVEFORMATEXTENSIBLE).read_unaligned() };
        let subtype = extensible.SubFormat;
        Some(subtype.to_u128())
    } else {
        None
    };
    let is_float = tag == 3 || subtype == Some(0x00000003_0000_0010_8000_00aa00389b71);
    let is_pcm = tag == 1 || subtype == Some(0x00000001_0000_0010_8000_00aa00389b71);
    match (is_float, is_pcm, bits) {
        (true, _, 32) => Ok(SampleFormat::Float32),
        (_, true, 16) => Ok(SampleFormat::Int16),
        (_, true, 24) => Ok(SampleFormat::Int24),
        (_, true, 32) => Ok(SampleFormat::Int32),
        _ => Err(format!(
            "The playback device uses an unsupported audio format (tag {tag}, {bits} bits)."
        )),
    }
}

fn decode_sample(bytes: &[u8], format: SampleFormat) -> f32 {
    match format {
        SampleFormat::Float32 => {
            f32::from_le_bytes(bytes[..4].try_into().unwrap()).clamp(-1.0, 1.0)
        }
        SampleFormat::Int16 => i16::from_le_bytes(bytes[..2].try_into().unwrap()) as f32 / 32768.0,
        SampleFormat::Int24 => {
            let value = i32::from_le_bytes([
                bytes[0],
                bytes[1],
                bytes[2],
                if bytes[2] & 0x80 != 0 { 0xff } else { 0 },
            ]);
            value as f32 / 8_388_608.0
        }
        SampleFormat::Int32 => {
            i32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32 / 2_147_483_648.0
        }
    }
}

struct Resampler {
    input_rate: u32,
    phase: u32,
    sum: f32,
    count: u32,
    packet: Vec<u8>,
}

impl Resampler {
    fn new(input_rate: u32) -> Self {
        Self {
            input_rate,
            phase: 0,
            sum: 0.0,
            count: 0,
            packet: Vec::with_capacity(PACKET_SAMPLES * 2),
        }
    }
    fn push(&mut self, sample: f32) -> Option<Vec<u8>> {
        self.sum += sample;
        self.count += 1;
        self.phase += OUTPUT_RATE;
        if self.phase < self.input_rate {
            return None;
        }
        self.phase -= self.input_rate;
        let value = (self.sum / self.count as f32).clamp(-1.0, 1.0);
        self.sum = 0.0;
        self.count = 0;
        self.packet
            .extend_from_slice(&((value * 32767.0) as i16).to_le_bytes());
        (self.packet.len() == PACKET_SAMPLES * 2)
            .then(|| std::mem::replace(&mut self.packet, Vec::with_capacity(PACKET_SAMPLES * 2)))
    }
}

fn capture_playback(
    stop: &AtomicBool,
    sender: &SyncSender<AudioEvent>,
    window: isize,
) -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
        .map_err(|error| format!("Could not initialize Windows audio: {error}"))?;
    let result = capture_playback_initialized(stop, sender, window);
    unsafe {
        CoUninitialize();
    }
    result
}

fn capture_playback_initialized(
    stop: &AtomicBool,
    sender: &SyncSender<AudioEvent>,
    window: isize,
) -> Result<(), String> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|error| format!("Could not find Windows audio devices: {error}"))?;
    let device =
        unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }.map_err(|error| {
            format!("Choose a default Windows output device before recording system audio: {error}")
        })?;
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|error| format!("Could not open system playback: {error}"))?;
    let mix = unsafe { client.GetMixFormat() }
        .map_err(|error| format!("Could not read playback format: {error}"))?;
    let format = unsafe { mix.read_unaligned() };
    let decoded = sample_format(mix);
    let rate = format.nSamplesPerSec;
    let channels = usize::from(format.nChannels);
    let block_align = usize::from(format.nBlockAlign);
    if rate < OUTPUT_RATE || channels == 0 || channels > 16 || block_align == 0 {
        unsafe {
            CoTaskMemFree(Some(mix.cast()));
        }
        return Err("The playback device has an unsupported sample rate or channel layout.".into());
    }
    let decoded = match decoded {
        Ok(decoded) => decoded,
        Err(error) => {
            unsafe {
                CoTaskMemFree(Some(mix.cast()));
            }
            return Err(error);
        }
    };
    let bytes_per_sample = match decoded {
        SampleFormat::Int16 => 2,
        SampleFormat::Int24 => 3,
        _ => 4,
    };
    if channels * bytes_per_sample > block_align {
        unsafe {
            CoTaskMemFree(Some(mix.cast()));
        }
        return Err("The playback device reported an invalid audio frame size.".into());
    }
    let initialized = unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            2_000_000,
            0,
            mix,
            None,
        )
    };
    unsafe {
        CoTaskMemFree(Some(mix.cast()));
    }
    initialized
        .map_err(|error| format!("Could not start Windows system-audio loopback: {error}"))?;
    let capture: IAudioCaptureClient = unsafe { client.GetService() }
        .map_err(|error| format!("Could not open the playback capture buffer: {error}"))?;
    unsafe { client.Start() }
        .map_err(|error| format!("Could not start playback capture: {error}"))?;
    notify(sender, window, AudioEvent::Ready);
    let mut resampler = Resampler::new(rate);
    let mut result = Ok(());
    while !stop.load(Ordering::Acquire) {
        let packet_size = match unsafe { capture.GetNextPacketSize() } {
            Ok(size) => size,
            Err(error) => {
                result = Err(format!("System audio capture stopped: {error}"));
                break;
            }
        };
        if packet_size == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let mut data = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        if let Err(error) =
            unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
        {
            result = Err(format!("Could not read system audio: {error}"));
            break;
        }
        for frame in 0..frames as usize {
            let sample = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                0.0
            } else {
                let offset = frame * block_align;
                let bytes = unsafe { std::slice::from_raw_parts(data.add(offset), block_align) };
                (0..channels)
                    .map(|channel| decode_sample(&bytes[channel * bytes_per_sample..], decoded))
                    .sum::<f32>()
                    / channels as f32
            };
            if let Some(packet) = resampler.push(sample) {
                notify(sender, window, AudioEvent::Samples(packet));
            }
        }
        if let Err(error) = unsafe { capture.ReleaseBuffer(frames) } {
            result = Err(format!("Could not release system audio: {error}"));
            break;
        }
    }
    let _ = unsafe { client.Stop() };
    result
}

#[cfg(test)]
mod tests {
    use super::{AudioEvent, MeetingLoopback, Resampler, SampleFormat, sample_format};
    use windows_startup::Win32::Media::Audio::{WAVEFORMATEX, WAVEFORMATEXTENSIBLE};
    #[test]
    fn resampler_keeps_ten_packets_per_second_at_common_output_rates() {
        for rate in [44_100, 48_000, 96_000] {
            let mut resampler = Resampler::new(rate);
            let packets = (0..rate).filter_map(|_| resampler.push(0.25)).count();
            assert_eq!(packets, 10);
        }
    }
    #[test]
    fn accepts_the_common_endpoint_mix_formats() {
        let pcm = WAVEFORMATEX {
            wFormatTag: 1,
            wBitsPerSample: 16,
            ..Default::default()
        };
        assert!(matches!(sample_format(&pcm), Ok(SampleFormat::Int16)));
        let mut extended = WAVEFORMATEXTENSIBLE::default();
        extended.Format.wFormatTag = 0xfffe;
        extended.Format.wBitsPerSample = 32;
        extended.Format.cbSize = 22;
        extended.SubFormat =
            windows_startup::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
        assert!(matches!(
            sample_format(std::ptr::addr_of!(extended.Format)),
            Ok(SampleFormat::Float32)
        ));
    }
    #[test]
    #[ignore = "requires an active Windows output device"]
    fn starts_and_stops_default_output_capture() {
        let capture = MeetingLoopback::start(0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match capture.next() {
                Some(AudioEvent::Ready) => return,
                Some(AudioEvent::Failed(error)) => panic!("{error}"),
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        panic!("Windows did not initialize loopback capture");
    }
}
