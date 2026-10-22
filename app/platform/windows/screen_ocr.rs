//! Local, bounded screen OCR using the Windows OCR engine available to the
//! packaged desktop app. The caller owns capture and sends one compressed
//! image; this module never retains screen pixels after recognition.

const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_DIMENSION: u32 = 3_840;
const MAX_TEXT_CHARS: usize = 64_000;

pub fn recognize_image(bytes: &[u8]) -> Result<String, String> {
    use windows_startup::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat};
    use windows_startup::Media::Ocr::OcrEngine;
    use windows_startup::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    use windows_startup::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("The captured image is empty or too large. Try a smaller screen area.".into());
    }
    let is_jpeg = bytes.starts_with(&[0xff, 0xd8, 0xff]);
    let is_png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    if !is_jpeg && !is_png {
        return Err("Island received an invalid screen image. Capture the area again.".into());
    }

    // OCR and bitmap decoding run on a worker thread so screen recognition
    // never blocks the native shell's animation or input loop.
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
        return Err("Windows OCR could not initialize its local imaging runtime.".into());
    }
    let _apartment = RoApartment;

    let stream = InMemoryRandomAccessStream::new()
        .map_err(|_| "Windows could not open the captured image.".to_owned())?;
    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|_| "Windows could not prepare the captured image for OCR.".to_owned())?;
    writer
        .WriteBytes(bytes)
        .map_err(|_| "Windows could not read the captured image.".to_owned())?;
    writer
        .StoreAsync()
        .and_then(|operation| operation.join())
        .map_err(|_| "Windows could not decode the captured image.".to_owned())?;
    stream
        .Seek(0)
        .map_err(|_| "Windows could not rewind the captured image.".to_owned())?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .and_then(|operation| operation.join())
        .map_err(|_| "The captured image format could not be decoded.".to_owned())?;
    let width = decoder
        .PixelWidth()
        .map_err(|_| "The captured image dimensions are unavailable.".to_owned())?;
    let height = decoder
        .PixelHeight()
        .map_err(|_| "The captured image dimensions are unavailable.".to_owned())?;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > 8_294_400
    {
        return Err("The captured image is outside the supported size. Select a smaller area.".into());
    }

    let bitmap = decoder
        .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Ignore)
        .and_then(|operation| operation.join())
        .map_err(|_| "Windows could not prepare the image for OCR.".to_owned())?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
        "Local Windows OCR is unavailable. Install a Windows OCR language pack and use the packaged Island app.".to_owned()
    })?;
    let operation = engine
        .RecognizeAsync(&bitmap)
        .map_err(|_| "Windows could not recognize text in this image.".to_owned())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        match operation
            .Status()
            .map_err(|_| "Windows OCR stopped before returning a result.".to_owned())?
            .0
        {
            1 => break,
            2 => return Err("Windows OCR was cancelled.".into()),
            3 => return Err("Windows OCR could not recognize text in this image.".into()),
            _ if std::time::Instant::now() >= deadline => {
                let _ = operation.Cancel();
                return Err("OCR took too long. Try a smaller screen area.".into());
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    let result = operation
        .GetResults()
        .map_err(|_| "Windows OCR did not return a result.".to_owned())?;
    let text = result
        .Text()
        .map_err(|_| "Windows OCR did not return text.".to_owned())?
        .to_string();
    if text.is_empty() {
        return Err("No text found in the selected area.".into());
    }
    Ok(text.chars().take(MAX_TEXT_CHARS).collect())
}

struct RoApartment;

impl Drop for RoApartment {
    fn drop(&mut self) {
        use windows_startup::Win32::System::WinRT::RoUninitialize;
        unsafe { RoUninitialize() };
    }
}
