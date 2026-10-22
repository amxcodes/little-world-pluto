//! Windows media capability probe for Phase 0.
//!
//! Reports active endpoint counts and Windows capture/OCR API availability,
//! then inspects H.264 transform type negotiation. OCR recognition is exercised
//! only against a synthetic in-memory bitmap. With `--synthetic-mp4`, it also
//! encodes generated frames and reads them back through Media Foundation. It
//! never records audio, opens a microphone stream, captures the screen, or
//! prints device names/IDs.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use windows::Win32::{
        Media::Audio::{
            IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eRender,
        },
        System::Com::{CLSCTX_ALL, CoCreateInstance},
    };
    use windows::{Graphics::Capture::GraphicsCaptureSession, Media::Ocr::OcrEngine};

    let _com = ComApartment::initialize()?;
    if let Some(package_full_name) = current_package_full_name() {
        return run_packaged_ocr_smoke(&package_full_name);
    }

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    let capture_count = active_endpoint_count(&enumerator, eCapture)?;
    let render_count = active_endpoint_count(&enumerator, eRender)?;
    let default_capture = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(eCapture, eCommunications)
            .is_ok()
    };

    let _media_foundation = MediaFoundationSession::initialize()?;
    let h264 = enumerate_h264_encoders()?;
    let run_synthetic_encode = std::env::args().any(|arg| arg == "--synthetic-mp4");

    let graphics_capture_supported = GraphicsCaptureSession::IsSupported();
    let ocr_languages = OcrEngine::AvailableRecognizerLanguages();
    let ocr_engine = OcrEngine::TryCreateFromUserProfileLanguages();

    println!("active_capture_endpoints={capture_count}");
    println!("active_render_endpoints={render_count}");
    println!("default_communications_capture_available={default_capture}");
    println!("h264_encoder_candidates={}", h264.candidates);
    println!("h264_encoder_activatable={}", h264.activatable);
    println!("h264_async_transforms={}", h264.async_transforms);
    println!("h264_async_transforms_unlocked={}", h264.async_unlocked);
    println!("h264_output_types_advertised={}", h264.output_type_count);
    println!("h264_output_types_configured={}", h264.output_configured);
    println!("h264_input_type_advertisements={}", h264.input_type_count);
    println!(
        "h264_output_type_query_errors={}",
        format_hresult_counts(&h264.output_query_errors)
    );
    println!(
        "h264_output_type_set_errors={}",
        format_hresult_counts(&h264.output_set_errors)
    );
    println!(
        "h264_input_type_query_errors={}",
        format_hresult_counts(&h264.input_query_errors)
    );
    println!(
        "h264_transforms_advertising_nv12_input={}",
        h264.nv12_transform_count
    );
    match graphics_capture_supported {
        Ok(supported) => println!("windows_graphics_capture_supported={supported}"),
        Err(error) => println!(
            "windows_graphics_capture_supported=unavailable; hresult=0x{:08X}",
            error.code().0 as u32
        ),
    }
    match &ocr_languages {
        Ok(languages) => println!("windows_ocr_language_count={}", languages.Size()?),
        Err(error) => println!(
            "windows_ocr_language_count=unavailable; hresult=0x{:08X}",
            error.code().0 as u32
        ),
    }
    match &ocr_engine {
        Ok(_) => println!("windows_ocr_user_profile_engine_available=true"),
        Err(error) => println!(
            "windows_ocr_user_profile_engine_available=false; hresult=0x{:08X}",
            error.code().0 as u32
        ),
    }
    if let Ok(engine) = ocr_engine {
        let (baseline_matches, fixture_matches, fixture_count) =
            recognize_synthetic_ocr_corpus(&engine);
        println!("windows_ocr_synthetic_text_match={baseline_matches}");
        println!("windows_ocr_fixture_pass_count={fixture_matches}/{fixture_count}");
    } else {
        println!("windows_ocr_synthetic_recognition=unavailable; reason=engine_creation_failed");
    }
    if run_synthetic_encode {
        let output = std::env::current_dir()?
            .join("target")
            .join(format!("phase0-synthetic-mp4-{}.mp4", std::process::id()));
        let started = std::time::Instant::now();
        let report = encode_synthetic_mp4(&output)?;
        println!("synthetic_mp4_path={}", output.display());
        println!("synthetic_mp4_bytes={}", report.bytes);
        println!("synthetic_mp4_frames_submitted={}", report.submitted_frames);
        println!("synthetic_mp4_frames_decoded={}", report.decoded_frames);
        println!(
            "synthetic_mp4_encode_and_decode_ms={}",
            started.elapsed().as_millis()
        );
        println!(
            "synthetic_mp4_container_boxes={}",
            report.container_boxes.join(",")
        );
        println!("scope=synthetic_in_memory_video_only; no_user_media_no_capture_no_audio_samples");
    } else {
        println!(
            "scope=capability_enumeration_local_mft_type_negotiation_and_synthetic_in_memory_ocr; no_user_media_no_capture_no_audio_samples_no_video_samples"
        );
    }
    Ok(())
}

#[cfg(windows)]
fn current_package_full_name() -> Option<String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

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
    if status != ERROR_INSUFFICIENT_BUFFER || length < 2 {
        return None;
    }

    let mut buffer = vec![0_u16; length as usize];
    let status = unsafe { GetCurrentPackageFullName(&mut length, buffer.as_mut_ptr()) };
    if status != 0 || length < 2 {
        return None;
    }

    Some(
        OsString::from_wide(&buffer[..length.saturating_sub(1) as usize])
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(windows)]
fn run_packaged_ocr_smoke(package_full_name: &str) -> windows::core::Result<()> {
    use windows::Media::Ocr::OcrEngine;

    let outcome = (|| {
        let language_count = OcrEngine::AvailableRecognizerLanguages()?.Size()?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let started = std::time::Instant::now();
        let (baseline_matches, fixture_matches, fixture_count) =
            recognize_synthetic_ocr_corpus(&engine);
        if !baseline_matches || fixture_matches != fixture_count {
            return Err(hresult_error(0x8000_4005));
        }
        Ok::<_, windows::core::Error>((
            language_count,
            fixture_matches,
            fixture_count,
            started.elapsed().as_millis(),
        ))
    })();

    let marker = std::env::temp_dir().join(format!("island-phase0-msix-{package_full_name}.txt"));
    match outcome {
        Ok((language_count, fixture_matches, fixture_count, elapsed_ms)) => {
            std::fs::write(
                &marker,
                format!(
                    "PASS {package_full_name}; LANGUAGES={language_count}; OCR=TEST; FIXTURES={fixture_matches}/{fixture_count}; ELAPSED_MS={elapsed_ms}"
                ),
            )?;
            println!("packaged_ocr_marker={}", marker.display());
        }
        Err(error) => {
            let failure_marker = std::env::temp_dir().join("island-phase0-msix-failed.txt");
            std::fs::write(
                &failure_marker,
                format!(
                    "FAIL packaged OCR; HRESULT=0x{:08X}; {error}",
                    error.code().0 as u32
                ),
            )?;
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(windows)]
struct SyntheticEncodeReport {
    bytes: u64,
    submitted_frames: u32,
    decoded_frames: u32,
    container_boxes: Vec<String>,
}

#[cfg(windows)]
fn encode_synthetic_mp4(path: &std::path::Path) -> windows::core::Result<SyntheticEncodeReport> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Media::MediaFoundation::{
        MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
        MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MFCreateMediaType,
        MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
        MFCreateSourceReaderFromURL, MFMediaType_Video, MFVideoFormat_H264, MFVideoFormat_NV12,
        MFVideoInterlace_Progressive,
    };
    use windows::core::PCWSTR;

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;
    const FRAME_RATE: u32 = 15;
    const FRAME_COUNT: u32 = FRAME_RATE * 2;
    const TICKS_PER_SECOND: i64 = 10_000_000;
    const FRAME_DURATION: i64 = TICKS_PER_SECOND / FRAME_RATE as i64;
    const NV12_BYTES: usize = (WIDTH * HEIGHT * 3 / 2) as usize;

    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent).map_err(|_| hresult_error(0x8007_0006))?;
    if path.exists() {
        return Err(hresult_error(0x8007_0050));
    }
    let output_wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let output_url = PCWSTR(output_wide.as_ptr());

    let target = unsafe { MFCreateMediaType()? };
    unsafe {
        target.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        target.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        target.SetUINT32(&MF_MT_AVG_BITRATE, 1_000_000)?;
        target.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        target.SetUINT64(&MF_MT_FRAME_SIZE, pack_ratio(WIDTH, HEIGHT))?;
        target.SetUINT64(&MF_MT_FRAME_RATE, pack_ratio(FRAME_RATE, 1))?;
        target.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))?;
    }

    let input = unsafe { MFCreateMediaType()? };
    unsafe {
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        input.SetUINT64(&MF_MT_FRAME_SIZE, pack_ratio(WIDTH, HEIGHT))?;
        input.SetUINT64(&MF_MT_FRAME_RATE, pack_ratio(FRAME_RATE, 1))?;
        input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))?;
    }

    let writer = unsafe { MFCreateSinkWriterFromURL(output_url, None, None)? };
    let stream = unsafe { writer.AddStream(&target)? };
    unsafe {
        writer.SetInputMediaType(stream, &input, None)?;
        writer.BeginWriting()?;
    }

    for frame_index in 0..FRAME_COUNT {
        let mut pixels = vec![128_u8; NV12_BYTES];
        let y_plane = &mut pixels[..(WIDTH * HEIGHT) as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let moving_bar = (x + frame_index * 7) % WIDTH < 48;
                y_plane[(y * WIDTH + x) as usize] = if moving_bar {
                    200
                } else {
                    48 + (y % 160) as u8
                };
            }
        }

        let buffer = unsafe { MFCreateMemoryBuffer(NV12_BYTES as u32)? };
        let mut destination = std::ptr::null_mut();
        unsafe { buffer.Lock(&mut destination, None, None)? };
        if destination.is_null() {
            unsafe { buffer.Unlock()? };
            return Err(hresult_error(0x8000_4003));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), destination, pixels.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(NV12_BYTES as u32)?;
        }

        let sample = unsafe { MFCreateSample()? };
        unsafe {
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(i64::from(frame_index) * FRAME_DURATION)?;
            sample.SetSampleDuration(FRAME_DURATION)?;
            writer.WriteSample(stream, &sample)?;
        }
    }
    unsafe { writer.Finalize()? };

    let metadata = std::fs::metadata(path).map_err(|_| hresult_error(0x8007_0006))?;
    if metadata.len() < 1024 || metadata.len() > 50 * 1024 * 1024 {
        return Err(hresult_error(0x8007_0006));
    }
    let bytes = std::fs::read(path).map_err(|_| hresult_error(0x8007_0006))?;
    let container_boxes = mp4_top_level_boxes(&bytes)?;
    for required in ["ftyp", "mdat", "moov"] {
        if !container_boxes.iter().any(|name| name == required) {
            return Err(hresult_error(0x8007_0006));
        }
    }

    let reader = unsafe { MFCreateSourceReaderFromURL(output_url, None)? };
    let mut decoded_frames = 0;
    for _ in 0..FRAME_COUNT + 4 {
        let mut flags = 0_u32;
        let mut sample = None;
        unsafe {
            reader.ReadSample(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                0,
                None,
                Some(&mut flags),
                None,
                Some(&mut sample),
            )?;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        if sample.is_some() {
            decoded_frames += 1;
        }
    }
    if decoded_frames != FRAME_COUNT {
        return Err(hresult_error(0x8007_0006));
    }

    Ok(SyntheticEncodeReport {
        bytes: metadata.len(),
        submitted_frames: FRAME_COUNT,
        decoded_frames,
        container_boxes,
    })
}

#[cfg(windows)]
fn pack_ratio(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(windows)]
fn hresult_error(code: u32) -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT(code as i32))
}

#[cfg(windows)]
fn mp4_top_level_boxes(bytes: &[u8]) -> windows::core::Result<Vec<String>> {
    let mut boxes = Vec::new();
    let mut offset = 0_usize;
    while offset + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as u64;
        let name = String::from_utf8_lossy(&bytes[offset + 4..offset + 8]).to_string();
        let (box_size, header_size) = if size == 1 {
            if offset + 16 > bytes.len() {
                return Err(hresult_error(0x8007_0006));
            }
            (
                u64::from_be_bytes(bytes[offset + 8..offset + 16].try_into().unwrap()),
                16_u64,
            )
        } else if size == 0 {
            ((bytes.len() - offset) as u64, 8_u64)
        } else {
            (size, 8_u64)
        };
        if box_size < header_size || box_size > (bytes.len() - offset) as u64 {
            return Err(hresult_error(0x8007_0006));
        }
        boxes.push(name);
        offset += box_size as usize;
    }
    if offset != bytes.len() {
        return Err(hresult_error(0x8007_0006));
    }
    Ok(boxes)
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct SyntheticOcrFixture {
    name: &'static str,
    scale: usize,
    background: [u8; 4],
    foreground: [u8; 4],
}

#[cfg(windows)]
const OCR_BASELINE: SyntheticOcrFixture = SyntheticOcrFixture {
    name: "baseline",
    scale: 8,
    background: [255, 255, 255, 255],
    foreground: [0, 0, 0, 255],
};

#[cfg(windows)]
const OCR_FIXTURES: [SyntheticOcrFixture; 5] = [
    OCR_BASELINE,
    SyntheticOcrFixture {
        name: "small",
        scale: 4,
        ..OCR_BASELINE
    },
    SyntheticOcrFixture {
        name: "large",
        scale: 12,
        ..OCR_BASELINE
    },
    SyntheticOcrFixture {
        name: "dark_theme",
        background: [0, 0, 0, 255],
        foreground: [255, 255, 255, 255],
        ..OCR_BASELINE
    },
    SyntheticOcrFixture {
        name: "low_contrast",
        background: [224, 224, 224, 255],
        foreground: [80, 80, 80, 255],
        ..OCR_BASELINE
    },
];

#[cfg(windows)]
fn recognize_synthetic_ocr_corpus(engine: &windows::Media::Ocr::OcrEngine) -> (bool, usize, usize) {
    let mut baseline_matches = false;
    let mut fixture_matches = 0;
    for fixture in OCR_FIXTURES {
        let started = std::time::Instant::now();
        match recognize_synthetic_text(engine, fixture) {
            Ok(recognized) => {
                let normalized = recognized.split_whitespace().collect::<Vec<_>>().join(" ");
                let matches = normalized.eq_ignore_ascii_case("TEST");
                if fixture.name == OCR_BASELINE.name {
                    baseline_matches = matches;
                    println!("windows_ocr_synthetic_text={normalized:?}");
                    println!(
                        "windows_ocr_synthetic_recognition_ms={}",
                        started.elapsed().as_millis()
                    );
                }
                if matches {
                    fixture_matches += 1;
                }
                println!(
                    "windows_ocr_fixture_{}_match={matches}; text={normalized:?}; elapsed_ms={}",
                    fixture.name,
                    started.elapsed().as_millis()
                );
            }
            Err(error) => println!(
                "windows_ocr_fixture_{}_error=0x{:08X}",
                fixture.name,
                error.code().0 as u32
            ),
        }
    }
    (baseline_matches, fixture_matches, OCR_FIXTURES.len())
}

#[cfg(windows)]
fn recognize_synthetic_text(
    engine: &windows::Media::Ocr::OcrEngine,
    fixture: SyntheticOcrFixture,
) -> windows::core::Result<String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Storage::Streams::Buffer;
    use windows::Win32::System::WinRT::IBufferByteAccess;
    use windows::core::Interface;

    const WIDTH: usize = 320;
    const HEIGHT: usize = 128;
    let scale = fixture.scale;
    let mut pixels = vec![0_u8; WIDTH * HEIGHT * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&fixture.background);
    }
    let glyph_width = 5 * scale;
    let glyph_height = 7 * scale;
    let spacing = scale + 8;
    let text_width = 4 * glyph_width + 3 * spacing;
    let left = (WIDTH - text_width) / 2;
    let top = (HEIGHT - glyph_height) / 2;
    for (index, glyph) in "TEST".chars().enumerate() {
        let rows = match glyph {
            'E' => [
                0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
            ],
            'S' => [
                0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
            ],
            'T' => [
                0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
            ],
            _ => unreachable!("synthetic OCR fixture contains only declared glyphs"),
        };
        for (row_index, bits) in rows.into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                let start_x = left + index * (glyph_width + spacing) + column * scale;
                let start_y = top + row_index * scale;
                for y in start_y..start_y + scale {
                    for x in start_x..start_x + scale {
                        let pixel = (y * WIDTH + x) * 4;
                        pixels[pixel..pixel + 4].copy_from_slice(&fixture.foreground);
                    }
                }
            }
        }
    }

    let buffer = Buffer::Create(pixels.len() as u32)?;
    buffer.SetLength(pixels.len() as u32)?;
    let byte_access: IBufferByteAccess = buffer.cast()?;
    let destination = unsafe { byte_access.Buffer()? };
    if destination.is_null() {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x8000_4003_u32 as i32,
        )));
    }
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), destination, pixels.len()) };

    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        WIDTH as i32,
        HEIGHT as i32,
    )?;
    let operation = engine.RecognizeAsync(&bitmap)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        match operation.Status()?.0 {
            1 => break, // AsyncStatus::Completed
            3 => return Err(windows::core::Error::from_hresult(operation.ErrorCode()?)),
            2 => {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x80004004_u32 as i32,
                )));
            }
            _ if std::time::Instant::now() >= deadline => {
                operation.Cancel()?;
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x800705B4_u32 as i32,
                )));
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    }
    let result = operation.GetResults()?;
    Ok(result.Text()?.to_string())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("windows_media_probe only runs on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn active_endpoint_count(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    flow: windows::Win32::Media::Audio::EDataFlow,
) -> windows::core::Result<u32> {
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

    unsafe {
        enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?
            .GetCount()
    }
}

#[cfg(windows)]
fn enumerate_h264_encoders() -> windows::core::Result<H264EncoderInventory> {
    use std::ptr;

    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, IMFTransform, MF_E_NO_MORE_TYPES, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC,
        MF_TRANSFORM_ASYNC_UNLOCK, MFMediaType_Video, MFT_CATEGORY_VIDEO_ENCODER,
        MFT_ENUM_FLAG_ALL, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
        MFVideoFormat_NV12,
    };

    let output_type = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut raw_activations: *mut Option<IMFActivate> = ptr::null_mut();
    let mut candidate_count = 0_u32;

    let enumeration = unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_ALL,
            None,
            Some(&output_type),
            &mut raw_activations,
            &mut candidate_count,
        )
    };
    let activations = MftActivationList {
        raw: raw_activations,
        count: candidate_count,
    };
    enumeration?;

    let mut inventory = H264EncoderInventory {
        candidates: candidate_count,
        ..H264EncoderInventory::default()
    };
    for index in 0..candidate_count as usize {
        let activation = unsafe { (*activations.raw.add(index)).as_ref() };
        let Some(activation) = activation else {
            continue;
        };
        let Ok(transform) = (unsafe { activation.ActivateObject::<IMFTransform>() }) else {
            continue;
        };
        inventory.activatable += 1;

        if let Ok(attributes) = unsafe { transform.GetAttributes() }
            && unsafe { attributes.GetUINT32(&MF_TRANSFORM_ASYNC) }
                .is_ok_and(|is_async| is_async != 0)
        {
            inventory.async_transforms += 1;
            if unsafe { attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1) }.is_ok() {
                inventory.async_unlocked += 1;
            }
        }

        let mut output_configured = false;
        // Configure the first advertised output type so transforms that require
        // output-first negotiation can expose compatible input types. This probe
        // submits no media samples.
        for type_index in 0..64 {
            match unsafe { transform.GetOutputAvailableType(0, type_index) } {
                Ok(media_type) => {
                    inventory.output_type_count += 1;
                    match unsafe { transform.SetOutputType(0, &media_type, 0) } {
                        Ok(()) => {
                            output_configured = true;
                            inventory.output_configured += 1;
                            break;
                        }
                        Err(error) => {
                            record_hresult(&mut inventory.output_set_errors, error.code().0 as u32);
                        }
                    }
                }
                Err(error) if error.code() == MF_E_NO_MORE_TYPES => break,
                Err(error) => {
                    record_hresult(&mut inventory.output_query_errors, error.code().0 as u32);
                    break;
                }
            }
        }
        if !output_configured {
            continue;
        }

        let mut advertises_nv12 = false;
        // Keep the probe bounded in case a transform returns an endless type list.
        for type_index in 0..64 {
            match unsafe { transform.GetInputAvailableType(0, type_index) } {
                Ok(media_type) => {
                    inventory.input_type_count += 1;
                    if unsafe { media_type.GetGUID(&MF_MT_SUBTYPE) }
                        .is_ok_and(|subtype| subtype == MFVideoFormat_NV12)
                    {
                        advertises_nv12 = true;
                    }
                }
                Err(error) if error.code() == MF_E_NO_MORE_TYPES => break,
                Err(error) => {
                    record_hresult(&mut inventory.input_query_errors, error.code().0 as u32);
                    break;
                }
            }
        }
        if advertises_nv12 {
            inventory.nv12_transform_count += 1;
        }
    }
    Ok(inventory)
}

#[cfg(windows)]
fn record_hresult(errors: &mut std::collections::BTreeMap<u32, u32>, code: u32) {
    *errors.entry(code).or_default() += 1;
}

#[cfg(windows)]
fn format_hresult_counts(errors: &std::collections::BTreeMap<u32, u32>) -> String {
    if errors.is_empty() {
        return "none".to_owned();
    }
    errors
        .iter()
        .map(|(code, count)| format!("0x{code:08X}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(windows)]
#[derive(Default)]
struct H264EncoderInventory {
    candidates: u32,
    activatable: u32,
    async_transforms: u32,
    async_unlocked: u32,
    output_configured: u32,
    output_type_count: u32,
    input_type_count: u32,
    nv12_transform_count: u32,
    output_query_errors: std::collections::BTreeMap<u32, u32>,
    output_set_errors: std::collections::BTreeMap<u32, u32>,
    input_query_errors: std::collections::BTreeMap<u32, u32>,
}

#[cfg(windows)]
struct MftActivationList {
    raw: *mut Option<windows::Win32::Media::MediaFoundation::IMFActivate>,
    count: u32,
}

#[cfg(windows)]
impl Drop for MftActivationList {
    fn drop(&mut self) {
        use std::ffi::c_void;

        use windows::Win32::{Media::MediaFoundation::IMFActivate, System::Com::CoTaskMemFree};

        if self.raw.is_null() {
            return;
        }
        for index in 0..self.count as usize {
            let activation: *mut Option<IMFActivate> = unsafe { self.raw.add(index) };
            unsafe { drop((*activation).take()) };
        }
        unsafe { CoTaskMemFree(Some(self.raw.cast::<c_void>())) };
    }
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl ComApartment {
    fn initialize() -> windows::core::Result<Self> {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::Com::CoUninitialize() };
    }
}

#[cfg(windows)]
struct MediaFoundationSession;

#[cfg(windows)]
impl MediaFoundationSession {
    fn initialize() -> windows::core::Result<Self> {
        use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFStartup};

        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL)? };
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for MediaFoundationSession {
    fn drop(&mut self) {
        if let Err(error) = unsafe { windows::Win32::Media::MediaFoundation::MFShutdown() } {
            eprintln!("Media Foundation shutdown failed: {error}");
        }
    }
}
