use std::io::Cursor;

pub fn decode_image(bytes: &[u8]) -> Result<Option<String>, String> {
    if bytes.len() > 12 * 1024 * 1024 {
        return Err("The selection is too large. Select a smaller area.".into());
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| "Could not read the selected image.".to_owned())?;
    let size = reader.output_buffer_size().ok_or("The selected image is too large.")?;
    if size > 96 * 1024 * 1024 {
        return Err("The selected image is too large.".into());
    }
    let mut pixels = vec![0; size];
    let frame = reader.next_frame(&mut pixels).map_err(|_| "Could not read the selected image.".to_owned())?;
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 || width.saturating_mul(height) > 24_000_000 {
        return Err("Select a smaller image area.".into());
    }
    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        _ => return Err("Unsupported image format.".into()),
    };
    let mut gray = Vec::with_capacity(width * height);
    for pixel in pixels[..frame.buffer_size()].chunks_exact(channels) {
        let value = if channels <= 2 {
            pixel[0]
        } else {
            ((pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8) as u8
        };
        gray.push(value);
    }
    let mut scanner = quircs::Quirc::default();
    for candidate in scanner.identify(width, height, &gray) {
        let Ok(code) = candidate else { continue };
        let Ok(data) = code.decode() else { continue };
        if let Ok(text) = String::from_utf8(data.payload) {
            if !text.trim().is_empty() { return Ok(Some(text)); }
        }
    }
    Ok(None)
}
