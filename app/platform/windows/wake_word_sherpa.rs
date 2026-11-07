use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sentencepiece::SentencePieceProcessor;
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, OnlineStream};
use windows_startup::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, IAudioCaptureClient, IAudioClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eCapture,
    eCommunications,
};
use windows_startup::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};

const SAMPLE_RATE: u32 = 16_000;
const FRAME_SAMPLES: usize = 1_600;
const MIC_QUEUE: usize = 8;

enum CaptureEvent {
    Ready,
    Samples(Vec<f32>),
    Failed(String),
}

struct MicrophoneCapture {
    stop: Arc<AtomicBool>,
    events: Receiver<CaptureEvent>,
    worker: Option<JoinHandle<()>>,
}

impl MicrophoneCapture {
    fn start() -> Result<Self, String> {
        let (sender, events) = sync_channel(MIC_QUEUE);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("pluto-wake-microphone".into())
            .spawn(move || {
                if let Err(error) = capture_microphone(&worker_stop, &sender) {
                    let _ = sender.try_send(CaptureEvent::Failed(error));
                }
            })
            .map_err(|error| format!("Could not start the local microphone: {error}"))?;
        let capture = Self {
            stop,
            events,
            worker: Some(worker),
        };
        match capture.events.recv_timeout(Duration::from_secs(10)) {
            Ok(CaptureEvent::Ready) => Ok(capture),
            Ok(CaptureEvent::Failed(error)) => Err(error),
            Ok(CaptureEvent::Samples(_)) => Err("The microphone started in an invalid state.".into()),
            Err(_) => Err("Windows did not start the microphone in time. Check microphone access and the default input device.".into()),
        }
    }

    fn drain(&self, spotter: &KeywordSpotter, stream: &OnlineStream) -> Result<bool, String> {
        loop {
            match self.events.try_recv() {
                Ok(CaptureEvent::Samples(samples)) => {
                    stream.accept_waveform(SAMPLE_RATE as i32, &samples);
                    while spotter.is_ready(stream) {
                        spotter.decode(stream);
                        if spotter
                            .get_result(stream)
                            .is_some_and(|result| !result.keyword.is_empty())
                        {
                            spotter.reset(stream);
                            return Ok(true);
                        }
                    }
                }
                Ok(CaptureEvent::Failed(error)) => return Err(error),
                Ok(CaptureEvent::Ready) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return Ok(false),
            }
        }
    }
}

impl Drop for MicrophoneCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct SherpaListener {
    microphone: MicrophoneCapture,
    spotter: KeywordSpotter,
    stream: OnlineStream,
}

impl SherpaListener {
    pub fn new(phrase: &str, sensitivity: super::wake_word::Sensitivity) -> Result<Self, String> {
        let directory = super::wake_word_model::model_dir()
            .filter(|directory| {
                std::fs::read_to_string(directory.join(".pluto-model-version"))
                    .is_ok_and(|version| version.trim() == super::wake_word_model::MODEL_VERSION)
            })
            .ok_or("Install the Sherpa-ONNX wake model in Settings → Voice first.")?;
        let encoder = directory.join("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx");
        let decoder = directory.join("decoder-epoch-12-avg-2-chunk-16-left-64.onnx");
        let joiner = directory.join("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx");
        let tokens = directory.join("tokens.txt");
        let bpe = directory.join("bpe.model");
        if [&encoder, &decoder, &joiner, &tokens, &bpe]
            .iter()
            .any(|path| !path.is_file())
        {
            return Err(
                "The Sherpa-ONNX wake model is incomplete. Repair it in Settings → Voice.".into(),
            );
        }

        let tokenizer = SentencePieceProcessor::open(&bpe)
            .map_err(|error| format!("Could not load wake phrase tokenizer: {error}"))?;
        let normalized = phrase.to_uppercase();
        let pieces = tokenizer
            .encode(&normalized)
            .map_err(|error| format!("Could not tokenize the wake phrase: {error}"))?
            .into_iter()
            .map(|piece| piece.piece)
            .collect::<Vec<_>>();
        if pieces.is_empty() {
            return Err(
                "The selected wake phrase cannot be tokenized by the English model.".into(),
            );
        }
        let threshold = match sensitivity {
            super::wake_word::Sensitivity::Low => 0.4,
            super::wake_word::Sensitivity::Balanced => 0.25,
            super::wake_word::Sensitivity::High => 0.15,
        };
        // The English GigaSpeech KWS model uses BPE token sequences. Its
        // keyword file format takes the encoded pieces followed by optional
        // score/threshold values; `@raw phrase` is only required by the
        // phone/pinyin tokenizers.
        let keyword = format!("{} :1.0 #{threshold:.2}", pieces.join(" "));
        let mut config = KeywordSpotterConfig::default();
        config.model_config.transducer.encoder = Some(encoder.to_string_lossy().into_owned());
        config.model_config.transducer.decoder = Some(decoder.to_string_lossy().into_owned());
        config.model_config.transducer.joiner = Some(joiner.to_string_lossy().into_owned());
        config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
        config.model_config.provider = Some("cpu".into());
        config.model_config.num_threads = 1;
        config.keywords_buf = Some(keyword);
        let spotter = KeywordSpotter::create(&config)
            .ok_or("Sherpa-ONNX could not initialize the local keyword model.")?;
        let stream = spotter.create_stream();
        let microphone = MicrophoneCapture::start()?;
        Ok(Self {
            microphone,
            spotter,
            stream,
        })
    }

    pub fn matched(&mut self) -> Result<bool, String> {
        self.microphone.drain(&self.spotter, &self.stream)
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
    let value = unsafe { format.read_unaligned() };
    let subtype = if value.wFormatTag == 0xfffe && value.cbSize >= 22 {
        let extended = format as *const WAVEFORMATEXTENSIBLE;
        let subtype = unsafe { std::ptr::addr_of!((*extended).SubFormat).read_unaligned() };
        Some(subtype.to_u128())
    } else {
        None
    };
    let float = value.wFormatTag == 3 || subtype == Some(0x00000003_0000_0010_8000_00aa00389b71);
    let pcm = value.wFormatTag == 1 || subtype == Some(0x00000001_0000_0010_8000_00aa00389b71);
    match (float, pcm, value.wBitsPerSample) {
        (true, _, 32) => Ok(SampleFormat::Float32),
        (_, true, 16) => Ok(SampleFormat::Int16),
        (_, true, 24) => Ok(SampleFormat::Int24),
        (_, true, 32) => Ok(SampleFormat::Int32),
        _ => {
            let tag = value.wFormatTag;
            let bits = value.wBitsPerSample;
            Err(format!(
                "The microphone uses an unsupported audio format (tag {tag}, {bits} bits)."
            ))
        }
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
    frame: Vec<f32>,
}

impl Resampler {
    fn new(input_rate: u32) -> Self {
        Self {
            input_rate,
            phase: 0,
            sum: 0.0,
            count: 0,
            frame: Vec::with_capacity(FRAME_SAMPLES),
        }
    }
    fn push(&mut self, sample: f32) -> Option<Vec<f32>> {
        self.sum += sample;
        self.count += 1;
        self.phase += SAMPLE_RATE;
        if self.phase < self.input_rate {
            return None;
        }
        self.phase -= self.input_rate;
        self.frame
            .push((self.sum / self.count as f32).clamp(-1.0, 1.0));
        self.sum = 0.0;
        self.count = 0;
        (self.frame.len() == FRAME_SAMPLES)
            .then(|| std::mem::replace(&mut self.frame, Vec::with_capacity(FRAME_SAMPLES)))
    }
}

fn capture_microphone(stop: &AtomicBool, sender: &SyncSender<CaptureEvent>) -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
        .map_err(|error| format!("Could not initialize Windows microphone capture: {error}"))?;
    let result = capture_microphone_initialized(stop, sender);
    unsafe {
        CoUninitialize();
    }
    result
}

fn capture_microphone_initialized(
    stop: &AtomicBool,
    sender: &SyncSender<CaptureEvent>,
) -> Result<(), String> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|error| format!("Could not find a Windows microphone: {error}"))?;
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) }.map_err(
        |error| format!("Choose a default Windows microphone and allow microphone access: {error}"),
    )?;
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|error| format!("Could not open the default microphone: {error}"))?;
    let mix = unsafe { client.GetMixFormat() }
        .map_err(|error| format!("Could not read the microphone format: {error}"))?;
    let format = unsafe { mix.read_unaligned() };
    let decoded = sample_format(mix);
    let rate = format.nSamplesPerSec;
    let channels = usize::from(format.nChannels);
    let block_align = usize::from(format.nBlockAlign);
    let bytes_per_sample = match decoded {
        Ok(SampleFormat::Int16) => 2,
        Ok(SampleFormat::Int24) => 3,
        Ok(_) => 4,
        Err(_) => 0,
    };
    if rate < SAMPLE_RATE
        || channels == 0
        || channels > 16
        || block_align == 0
        || channels * bytes_per_sample > block_align
    {
        unsafe {
            CoTaskMemFree(Some(mix.cast()));
        }
        return Err(
            "The default microphone has an unsupported sample rate or channel layout.".into(),
        );
    }
    let decoded = match decoded {
        Ok(format) => format,
        Err(error) => {
            unsafe {
                CoTaskMemFree(Some(mix.cast()));
            }
            return Err(error);
        }
    };
    let initialized =
        unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 2_000_000, 0, mix, None) };
    unsafe {
        CoTaskMemFree(Some(mix.cast()));
    }
    initialized.map_err(|error| format!("Could not start microphone capture: {error}"))?;
    let capture: IAudioCaptureClient = unsafe { client.GetService() }
        .map_err(|error| format!("Could not open the microphone capture buffer: {error}"))?;
    unsafe { client.Start() }
        .map_err(|error| format!("Could not start the microphone: {error}"))?;
    let _ = sender.try_send(CaptureEvent::Ready);
    let mut resampler = Resampler::new(rate);
    while !stop.load(Ordering::Acquire) {
        let packet_size = unsafe { capture.GetNextPacketSize() }
            .map_err(|error| format!("Microphone capture stopped: {error}"))?;
        if packet_size == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let mut data = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
            .map_err(|error| format!("Could not read microphone audio: {error}"))?;
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
                if let Err(TrySendError::Disconnected(_)) =
                    sender.try_send(CaptureEvent::Samples(packet))
                {
                    let _ = unsafe { capture.ReleaseBuffer(frames) };
                    let _ = unsafe { client.Stop() };
                    return Ok(());
                }
            }
        }
        unsafe { capture.ReleaseBuffer(frames) }
            .map_err(|error| format!("Could not release the microphone capture buffer: {error}"))?;
    }
    let _ = unsafe { client.Stop() };
    Ok(())
}
