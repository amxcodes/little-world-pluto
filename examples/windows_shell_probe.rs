#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
static POINTER_TARGET_CLICK_COUNT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[cfg(windows)]
unsafe extern "system" fn pointer_target_window_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    if message == windows_sys::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP {
        POINTER_TARGET_CLICK_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

#[cfg(windows)]
fn create_pointer_target_window(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<windows_sys::Win32::Foundation::HWND, Box<dyn std::error::Error>> {
    use std::{iter::once, ptr::null_mut};

    use windows_sys::Win32::{
        Foundation::GetLastError,
        Graphics::Gdi::{COLOR_WINDOW, HBRUSH},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CreateWindowExW, RegisterClassW, SW_SHOWNOACTIVATE,
            SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos, ShowWindow, WNDCLASSW, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    };

    let class_name: Vec<u16> = "LittleworldPhase0PointerTarget"
        .encode_utf16()
        .chain(once(0))
        .collect();
    let title: Vec<u16> = "Phase 0 pointer target"
        .encode_utf16()
        .chain(once(0))
        .collect();
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if instance.is_null() {
        return Err(format!("GetModuleHandleW failed: {}", unsafe { GetLastError() }).into());
    }

    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(pointer_target_window_proc),
        hInstance: instance,
        hbrBackground: ((COLOR_WINDOW + 1) as usize) as HBRUSH,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(format!("RegisterClassW failed: {}", unsafe { GetLastError() }).into());
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if hwnd.is_null() {
        return Err(format!("CreateWindowExW target failed: {}", unsafe {
            GetLastError()
        })
        .into());
    }
    unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    if unsafe {
        SetWindowPos(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    } == 0
    {
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
        return Err(format!("SetWindowPos target failed: {}", unsafe { GetLastError() }).into());
    }
    Ok(hwnd)
}

#[cfg(windows)]
struct CursorRestore(windows_sys::Win32::Foundation::POINT);

#[cfg(windows)]
impl Drop for CursorRestore {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(self.0.x, self.0.y);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This probe only runs on Windows.");
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt, ptr::null_mut};

    use island_desktop::visual::{
        CALIBRATED_STATUS_WIDTH, FOLDED_HEIGHT, FOLDED_HOST_HEIGHT, ShapeMode, ShellLayout,
    };

    use windows_sys::Win32::{
        Foundation::{GetLastError, POINT, SIZE},
        Graphics::Gdi::{
            AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER,
            CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW,
            DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DeleteDC, DeleteObject, FF_DONTCARE,
            FW_SEMIBOLD, GetTextExtentPoint32W, HBITMAP, HDC, HGDIOBJ, OUT_DEFAULT_PRECIS,
            SelectObject, SetBkMode, SetTextAlign, SetTextColor, TA_LEFT, TA_TOP, TRANSPARENT,
            TextOutW,
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
                SetProcessDpiAwarenessContext,
            },
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
                SendInput,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetCursorPos,
                GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowDisplayAffinity,
                GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, PM_REMOVE, PeekMessageW,
                SM_CXSCREEN, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetCursorPos,
                SetWindowDisplayAffinity, SetWindowPos, ShowWindow, TranslateMessage, ULW_ALPHA,
                UpdateLayeredWindow, WDA_EXCLUDEFROMCAPTURE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WindowFromPhysicalPoint,
            },
        },
    };

    // Screenshot-calibrated folded status width; the shared renderer keeps
    // its source-derived dynamic width target separately.
    const PILL_WIDTH: f64 = CALIBRATED_STATUS_WIDTH;
    const PILL_HEIGHT: f64 = FOLDED_HEIGHT as f64;
    const NOTCH_EAR: f64 = island_desktop::visual::NOTCH_EAR as f64;
    // Preserve a small transparent host tail for the measured outer crop;
    // shape coverage and hit testing use the source's 30 px body height.
    const HOST_HEIGHT: f64 = FOLDED_HOST_HEIGHT;
    const CAPSULE_GAP: f64 = 8.0;
    const ANTIALIAS_SAMPLES: usize = 4;
    const HOVER_TEXT: &str = "7:04";

    #[derive(Clone, Copy)]
    struct Rgba {
        r: u8,
        g: u8,
        b: u8,
    }

    impl Rgba {
        const BATTERY: Self = Self {
            r: 52,
            g: 211,
            b: 153,
        };
        const DARK_ISLAND: Self = Self { r: 0, g: 0, b: 0 };
        const LIGHT_ISLAND: Self = Self {
            r: 245,
            g: 245,
            b: 245,
        };
        const DARK_INK: Self = Self {
            r: 255,
            g: 255,
            b: 255,
        };
        const LIGHT_INK: Self = Self {
            r: 23,
            g: 23,
            b: 26,
        };
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(once(0)).collect()
    }

    fn parse_range(
        args: &[String],
        name: &str,
        default: u8,
        minimum: u8,
        maximum: u8,
    ) -> Result<u8, String> {
        let Some(index) = args.iter().position(|arg| arg == name) else {
            return Ok(default);
        };
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{name} requires a value from {minimum} to {maximum}"))?
            .parse::<u8>()
            .map_err(|_| format!("{name} requires a value from {minimum} to {maximum}"))?;
        if !(minimum..=maximum).contains(&value) {
            return Err(format!("{name} must be between {minimum} and {maximum}"));
        }
        Ok(value)
    }

    fn inside_round_rect(
        point: (f64, f64),
        bounds: (f64, f64, f64, f64),
        radius: f64,
        round_top: bool,
    ) -> bool {
        let (x, y) = point;
        let (left, top, width, height) = bounds;
        if x < left || x >= left + width || y < top || y >= top + height {
            return false;
        }
        let local_x = x - left;
        let local_y = y - top;
        if round_top && local_y < radius {
            if local_x < radius {
                return (local_x - radius).powi(2) + (local_y - radius).powi(2) <= radius.powi(2);
            }
            if local_x > width - radius {
                return (local_x - (width - radius)).powi(2) + (local_y - radius).powi(2)
                    <= radius.powi(2);
            }
        }
        if local_y >= height - radius {
            if local_x < radius {
                return (local_x - radius).powi(2) + (local_y - (height - radius)).powi(2)
                    <= radius.powi(2);
            }
            if local_x > width - radius {
                return (local_x - (width - radius)).powi(2)
                    + (local_y - (height - radius)).powi(2)
                    <= radius.powi(2);
            }
        }
        true
    }

    fn blend_pixel(pixel: &mut [u8], color: Rgba, alpha: f64) {
        let source_alpha = alpha.clamp(0.0, 1.0);
        let destination_alpha = f64::from(pixel[3]) / 255.0;
        let out_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
        if out_alpha <= 0.0 {
            return;
        }
        for (offset, channel) in [(0, color.b), (1, color.g), (2, color.r)] {
            let premultiplied =
                f64::from(channel) * source_alpha + f64::from(pixel[offset]) * (1.0 - source_alpha);
            pixel[offset] = premultiplied.round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    fn paint_primitive(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        logical_scale: f64,
        color: Rgba,
        contains: impl FnMut(f64, f64) -> bool,
    ) {
        paint_primitive_opacity(pixels, width, height, logical_scale, color, 1.0, contains);
    }

    fn paint_primitive_opacity(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        logical_scale: f64,
        color: Rgba,
        opacity: f64,
        mut contains: impl FnMut(f64, f64) -> bool,
    ) {
        let sample_count = (ANTIALIAS_SAMPLES * ANTIALIAS_SAMPLES) as f64;
        for pixel_y in 0..height {
            for pixel_x in 0..width {
                let mut covered = 0usize;
                for sy in 0..ANTIALIAS_SAMPLES {
                    for sx in 0..ANTIALIAS_SAMPLES {
                        let logical_x = (pixel_x as f64
                            + (sx as f64 + 0.5) / ANTIALIAS_SAMPLES as f64)
                            / logical_scale;
                        let logical_y = (pixel_y as f64
                            + (sy as f64 + 0.5) / ANTIALIAS_SAMPLES as f64)
                            / logical_scale;
                        covered += usize::from(contains(logical_x, logical_y));
                    }
                }
                if covered != 0 {
                    let offset = (pixel_y * width + pixel_x) * 4;
                    blend_pixel(
                        &mut pixels[offset..offset + 4],
                        color,
                        covered as f64 / sample_count * opacity,
                    );
                }
            }
        }
    }

    fn point_in_polygon(point: (f64, f64), vertices: &[(f64, f64)]) -> bool {
        let (x, y) = point;
        let mut inside = false;
        let mut previous = vertices.len().saturating_sub(1);
        for current in 0..vertices.len() {
            let (xi, yi) = vertices[current];
            let (xj, yj) = vertices[previous];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            previous = current;
        }
        inside
    }

    fn distance_to_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
        let (px, py) = point;
        let (sx, sy) = start;
        let (ex, ey) = end;
        let dx = ex - sx;
        let dy = ey - sy;
        let length_squared = dx * dx + dy * dy;
        let projection = if length_squared == 0.0 {
            0.0
        } else {
            (((px - sx) * dx + (py - sy) * dy) / length_squared).clamp(0.0, 1.0)
        };
        (px - (sx + projection * dx)).hypot(py - (sy + projection * dy))
    }

    fn polygon_edge_distance(point: (f64, f64), vertices: &[(f64, f64)]) -> f64 {
        (0..vertices.len())
            .map(|index| {
                distance_to_segment(
                    point,
                    vertices[index],
                    vertices[(index + 1) % vertices.len()],
                )
            })
            .fold(f64::INFINITY, f64::min)
    }

    fn paint_shape(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        logical_scale: f64,
        color: Rgba,
        pill_size: (f64, f64),
        capsule: bool,
    ) {
        let shape = if capsule {
            ShapeMode::Capsule
        } else {
            ShapeMode::Notch
        };
        let layout =
            ShellLayout::new(shape, pill_size.0, 1.0).expect("probe shape dimensions are valid");
        let sample_count = (ANTIALIAS_SAMPLES * ANTIALIAS_SAMPLES) as f64;
        for pixel_y in 0..height {
            for pixel_x in 0..width {
                let mut coverage = 0.0;
                for sy in 0..ANTIALIAS_SAMPLES {
                    for sx in 0..ANTIALIAS_SAMPLES {
                        let logical_x = (pixel_x as f64
                            + (sx as f64 + 0.5) / ANTIALIAS_SAMPLES as f64)
                            / logical_scale;
                        let logical_y = (pixel_y as f64
                            + (sy as f64 + 0.5) / ANTIALIAS_SAMPLES as f64)
                            / logical_scale;
                        coverage += layout.coverage(logical_x, logical_y);
                    }
                }
                if coverage > 0.0 {
                    let offset = (pixel_y * width + pixel_x) * 4;
                    blend_pixel(
                        &mut pixels[offset..offset + 4],
                        color,
                        coverage / sample_count,
                    );
                }
            }
        }
    }

    fn draw_text(
        pixels: &mut [u8],
        size: (usize, usize),
        scale: f64,
        color: Rgba,
        origin: (f64, f64),
        text: &str,
    ) {
        let (width, height) = size;
        if width == 0 || height == 0 || width > i32::MAX as usize || height > i32::MAX as usize {
            return;
        }

        let dc: HDC = unsafe { CreateCompatibleDC(null_mut()) };
        if dc.is_null() {
            return;
        }
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut mask_pixels = null_mut();
        let mask: HBITMAP = unsafe {
            CreateDIBSection(
                dc,
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut mask_pixels,
                null_mut(),
                0,
            )
        };
        if mask.is_null() || mask_pixels.is_null() {
            if !mask.is_null() {
                unsafe { DeleteObject(mask as HGDIOBJ) };
            }
            unsafe { DeleteDC(dc) };
            return;
        }
        let previous_bitmap: HGDIOBJ = unsafe { SelectObject(dc, mask as HGDIOBJ) };
        if previous_bitmap.is_null() {
            unsafe {
                DeleteObject(mask as HGDIOBJ);
                DeleteDC(dc);
            }
            return;
        }
        unsafe { std::ptr::write_bytes(mask_pixels.cast::<u8>(), 0, pixels.len()) };

        let face = wide(OsStr::new("Segoe UI Variable Display"));
        let font_height = (11.0 * scale).round().max(1.0) as i32;
        let font = unsafe {
            CreateFontW(
                -font_height,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_DEFAULT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(ANTIALIASED_QUALITY),
                u32::from(DEFAULT_PITCH) | u32::from(FF_DONTCARE),
                face.as_ptr(),
            )
        };
        if font.is_null() {
            unsafe {
                SelectObject(dc, previous_bitmap);
                DeleteObject(mask as HGDIOBJ);
                DeleteDC(dc);
            }
            return;
        }
        let previous_font: HGDIOBJ = unsafe { SelectObject(dc, font as HGDIOBJ) };
        if previous_font.is_null() {
            unsafe {
                DeleteObject(font as HGDIOBJ);
                SelectObject(dc, previous_bitmap);
                DeleteObject(mask as HGDIOBJ);
                DeleteDC(dc);
            }
            return;
        }

        let wide_text = wide(OsStr::new(text));
        let mut text_size = SIZE::default();
        let measured = unsafe {
            GetTextExtentPoint32W(
                dc,
                wide_text.as_ptr(),
                wide_text.len().saturating_sub(1) as i32,
                &mut text_size,
            )
        };
        if measured != 0 {
            unsafe {
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextAlign(dc, TA_LEFT | TA_TOP);
                SetTextColor(dc, 0x00FF_FFFF);
                TextOutW(
                    dc,
                    (origin.0 * scale).round() as i32,
                    ((origin.1 * scale).round() as i32 - text_size.cy / 2).max(0),
                    wide_text.as_ptr(),
                    wide_text.len().saturating_sub(1) as i32,
                );
            }

            let mask_pixels =
                unsafe { std::slice::from_raw_parts(mask_pixels.cast::<u8>(), pixels.len()) };
            for (source, target) in mask_pixels.chunks_exact(4).zip(pixels.chunks_exact_mut(4)) {
                let coverage = f64::from(source[0].max(source[1]).max(source[2])) / 255.0;
                if coverage > 0.0 {
                    blend_pixel(target, color, coverage);
                }
            }
        }

        unsafe {
            SelectObject(dc, previous_font);
            DeleteObject(font as HGDIOBJ);
            SelectObject(dc, previous_bitmap);
            DeleteObject(mask as HGDIOBJ);
            DeleteDC(dc);
        }
    }

    fn render(
        pixel_size: (usize, usize),
        scale: f64,
        pill_size: (f64, f64),
        capsule: bool,
        light: bool,
        transparency_percent: u8,
    ) -> Vec<u8> {
        let (width, height) = pixel_size;
        let (pill_width, pill_height) = pill_size;
        let mut pixels = vec![0; width * height * 4];
        let background = if light {
            Rgba::LIGHT_ISLAND
        } else {
            Rgba::DARK_ISLAND
        };
        let ink = if light {
            Rgba::LIGHT_INK
        } else {
            Rgba::DARK_INK
        };
        let opacity = 1.0 - f64::from(transparency_percent) / 100.0;

        paint_shape(
            &mut pixels,
            width,
            height,
            scale,
            background,
            (pill_width, pill_height),
            capsule,
        );

        let main_left = if capsule { 0.0 } else { NOTCH_EAR };
        let content_start = main_left + 52.0;
        let text_y = pill_height / 2.0;
        let cloud_color = ink;
        paint_primitive(&mut pixels, width, height, scale, cloud_color, |x, y| {
            let origin_x = main_left + 10.0;
            let origin_y = (pill_height - 9.0) / 2.0;
            let cloud = (x - (origin_x + 4.0)).hypot(y - (origin_y + 5.0)) <= 3.4
                || (x - (origin_x + 7.0)).hypot(y - (origin_y + 3.8)) <= 2.7
                || (x - (origin_x + 9.0)).hypot(y - (origin_y + 5.3)) <= 2.2;
            cloud
                || (x >= origin_x + 2.0
                    && x <= origin_x + 10.0
                    && y >= origin_y + 5.0
                    && y <= origin_y + 7.0)
        });
        draw_text(
            &mut pixels,
            (width, height),
            scale,
            ink,
            (content_start, text_y),
            HOVER_TEXT,
        );

        let battery_x = content_start + 31.0;
        let battery_y = (pill_height - 11.0) / 2.0;
        paint_primitive_opacity(&mut pixels, width, height, scale, ink, 0.45, |x, y| {
            inside_round_rect((x, y), (battery_x, battery_y, 19.0, 11.0), 3.0, true)
                && !inside_round_rect(
                    (x, y),
                    (battery_x + 1.0, battery_y + 1.0, 17.0, 9.0),
                    2.5,
                    true,
                )
        });
        paint_primitive_opacity(&mut pixels, width, height, scale, ink, 0.45, |x, y| {
            inside_round_rect(
                (x, y),
                (battery_x + 19.6, battery_y + 3.5, 1.9, 4.0),
                0.95,
                true,
            )
        });
        paint_primitive(&mut pixels, width, height, scale, Rgba::BATTERY, |x, y| {
            inside_round_rect(
                (x, y),
                (battery_x + 2.0, battery_y + 2.0, 15.0, 7.0),
                1.6,
                true,
            )
        });

        let bolt = [
            (battery_x + 10.6, battery_y + 1.4),
            (battery_x + 6.9, battery_y + 6.1),
            (battery_x + 9.2, battery_y + 6.1),
            (battery_x + 8.4, battery_y + 9.6),
            (battery_x + 12.1, battery_y + 4.9),
            (battery_x + 9.8, battery_y + 4.9),
        ];
        paint_primitive(&mut pixels, width, height, scale, ink, |x, y| {
            point_in_polygon((x, y), &bolt)
        });
        paint_primitive(&mut pixels, width, height, scale, background, |x, y| {
            point_in_polygon((x, y), &bolt) && polygon_edge_distance((x, y), &bolt) <= 0.35
        });

        let status_x = battery_x + 23.2;
        draw_text(
            &mut pixels,
            (width, height),
            scale,
            ink,
            (status_x, text_y),
            "100%",
        );
        draw_text(
            &mut pixels,
            (width, height),
            scale,
            ink,
            (status_x + 57.8, text_y),
            "29°",
        );

        if transparency_percent != 0 {
            for pixel in pixels.chunks_exact_mut(4) {
                let alpha = (f64::from(pixel[3]) * opacity).round() as u8;
                for channel in &mut pixel[..3] {
                    *channel = (f64::from(*channel) * opacity).round() as u8;
                }
                pixel[3] = alpha;
            }
        }
        pixels
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let capsule = args.iter().any(|arg| arg == "--capsule");
    let light = args.iter().any(|arg| arg == "--light");
    let offset_left = args.iter().any(|arg| arg == "--left");
    let exclude_from_capture = args.iter().any(|arg| arg == "--exclude-from-capture");
    let verify_hover = args.iter().any(|arg| arg == "--verify-hover");
    let verify_hit_map = args.iter().any(|arg| arg == "--verify-hitmap") || verify_hover;
    let verify_pointer = args.iter().any(|arg| arg == "--verify-pointer");
    let scale_percent = parse_range(&args, "--scale", 100, 80, 200)?;
    let transparency_percent = parse_range(&args, "--transparency", 0, 0, 100)?;
    let shape = if capsule { "Capsule" } else { "Notch" };
    let window_title = format!("Island Windows shell probe — {shape}");
    let title = wide(OsStr::new(&window_title));
    let class_name = wide(OsStr::new("STATIC"));

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let dpi = match unsafe { GetDpiForSystem() } {
        0 => 96,
        value => value,
    };
    let logical_scale = f64::from(dpi) / 96.0 * f64::from(scale_percent) / 100.0;
    let pill_width = PILL_WIDTH;
    let pill_height = PILL_HEIGHT;
    let logical_width = if capsule {
        pill_width
    } else {
        pill_width + 2.0 * NOTCH_EAR
    };
    let width = (logical_width * logical_scale).round() as i32;
    let height = (HOST_HEIGHT * logical_scale).round() as i32;
    let x = if offset_left {
        (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 4
    } else {
        (unsafe { GetSystemMetrics(SM_CXSCREEN) } - width) / 2
    };
    let y = if capsule {
        (CAPSULE_GAP * logical_scale).round() as i32
    } else {
        0
    };
    let foreground_before = unsafe { GetForegroundWindow() };
    let pointer_target = if verify_pointer {
        Some(create_pointer_target_window(x, 0, width, height.max(80))?)
    } else {
        None
    };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    if hwnd.is_null() {
        return Err(format!("CreateWindowExW failed: {}", unsafe { GetLastError() }).into());
    }

    let source_dc: HDC = unsafe { CreateCompatibleDC(null_mut()) };
    if source_dc.is_null() {
        return Err(format!("CreateCompatibleDC failed: {}", unsafe { GetLastError() }).into());
    }
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut raw_pixels = null_mut();
    let bitmap: HBITMAP = unsafe {
        CreateDIBSection(
            source_dc,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut raw_pixels,
            null_mut(),
            0,
        )
    };
    if bitmap.is_null() || raw_pixels.is_null() {
        unsafe { DeleteDC(source_dc) };
        return Err(format!("CreateDIBSection failed: {}", unsafe { GetLastError() }).into());
    }
    let previous_bitmap: HGDIOBJ = unsafe { SelectObject(source_dc, bitmap as HGDIOBJ) };
    if previous_bitmap.is_null() {
        unsafe {
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(source_dc);
        }
        return Err(format!("SelectObject failed: {}", unsafe { GetLastError() }).into());
    }

    let pixels = render(
        (width as usize, height as usize),
        logical_scale,
        (pill_width, pill_height),
        capsule,
        light,
        transparency_percent,
    );
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), raw_pixels.cast::<u8>(), pixels.len());
    }
    let destination = POINT { x, y };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT { x: 0, y: 0 };
    let blend = windows_sys::Win32::Graphics::Gdi::BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    if unsafe {
        UpdateLayeredWindow(
            hwnd,
            null_mut(),
            &destination,
            &size,
            source_dc,
            &source,
            0,
            &blend,
            ULW_ALPHA,
        )
    } == 0
    {
        unsafe {
            SelectObject(source_dc, previous_bitmap);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(source_dc);
        }
        return Err(format!("UpdateLayeredWindow failed: {}", unsafe { GetLastError() }).into());
    }
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }

    if exclude_from_capture
        && unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } == 0
    {
        eprintln!("Capture exclusion request failed: {}", unsafe {
            GetLastError()
        });
    }

    let mut rect = windows_sys::Win32::Foundation::RECT::default();
    let rect_ok = unsafe { GetWindowRect(hwnd, &mut rect) } != 0;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    let foreground_after = unsafe { GetForegroundWindow() };
    let mut affinity = 0;
    let affinity_ok = unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) } != 0;
    println!(
        "Window bounds: left={}, top={}, right={}, bottom={} (readback: {rect_ok})",
        rect.left, rect.top, rect.right, rect.bottom
    );
    println!("Window handle: {:#x}", hwnd as usize);
    println!(
        "Shape: {shape}; sample status pill: {pill_width:.0}x{pill_height:.0} logical px (150px minimum); notch ears: 14x14 logical px"
    );
    println!(
        "DPI: {dpi}; UI scale: {scale_percent}%; folded transparency: {transparency_percent}%; theme: {}",
        if light { "Light" } else { "Dark" }
    );
    println!(
        "Extended styles: layered={}, no-activate={}, topmost={}, tool-window={}",
        ex_style & WS_EX_LAYERED != 0,
        ex_style & WS_EX_NOACTIVATE != 0,
        ex_style & WS_EX_TOPMOST != 0,
        ex_style & WS_EX_TOOLWINDOW != 0,
    );
    println!(
        "Foreground unchanged after show: {} (before={:#x}, after={:#x})",
        foreground_before == foreground_after,
        foreground_before as usize,
        foreground_after as usize,
    );
    println!("Display affinity: {affinity:#x} (readback: {affinity_ok})");
    if verify_hit_map || verify_hover || verify_pointer {
        if !rect_ok {
            unsafe {
                DestroyWindow(hwnd);
                if let Some(target) = pointer_target {
                    DestroyWindow(target);
                }
                SelectObject(source_dc, previous_bitmap);
                DeleteObject(bitmap as HGDIOBJ);
                DeleteDC(source_dc);
            }
            return Err("GetWindowRect failed before hit-map verification".into());
        }

        let scan_width = width as usize;
        let scan_height = height as usize;
        let mut mismatches = 0usize;
        if verify_hit_map {
            let scan_hwnd = hwnd as usize;
            let scan_pixels = pixels.clone();
            let scan = std::thread::spawn(move || {
                let mut opaque_pixels = 0usize;
                let mut mismatches = 0usize;
                let mut first_mismatches = Vec::new();
                for pixel_y in 0..scan_height {
                    for pixel_x in 0..scan_width {
                        let offset = (pixel_y * scan_width + pixel_x) * 4;
                        let expected_probe_hit = scan_pixels[offset + 3] != 0;
                        opaque_pixels += usize::from(expected_probe_hit);
                        let point = POINT {
                            x: rect.left + pixel_x as i32,
                            y: rect.top + pixel_y as i32,
                        };
                        let actual_probe_window = unsafe { WindowFromPhysicalPoint(point) };
                        let actual_probe_hit = actual_probe_window as usize == scan_hwnd;
                        if expected_probe_hit != actual_probe_hit {
                            mismatches += 1;
                            if first_mismatches.len() < 8 {
                                first_mismatches.push((
                                    point.x,
                                    point.y,
                                    expected_probe_hit,
                                    actual_probe_window as usize,
                                ));
                            }
                        }
                    }
                }
                (opaque_pixels, mismatches, first_mismatches)
            });
            while !scan.is_finished() {
                let mut pending_message = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut pending_message, null_mut(), 0, 0, PM_REMOVE) }
                    != 0
                {
                    unsafe {
                        TranslateMessage(&pending_message);
                        DispatchMessageW(&pending_message);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let (opaque_pixels, scan_mismatches, first_mismatches) = scan
                .join()
                .map_err(|_| std::io::Error::other("hit-map scan thread panicked"))?;
            mismatches = scan_mismatches;
            println!(
                "Per-pixel WindowFromPhysicalPoint scan: total={}, nontransparent={}, mismatches={mismatches}",
                width as usize * height as usize,
                opaque_pixels
            );
            for (point_x, point_y, expected_probe_hit, actual_probe_handle) in first_mismatches {
                println!(
                    "Hit-map mismatch at ({point_x},{point_y}): expected probe hit={expected_probe_hit}, actual HWND={actual_probe_handle:#x}"
                );
            }
        }

        let hover_result = if verify_hover {
            let center_x = scan_width / 2;
            let center_y = (0..scan_height)
                .filter(|pixel_y| pixels[(pixel_y * scan_width + center_x) * 4 + 3] != 0)
                .min_by_key(|pixel_y| pixel_y.abs_diff(scan_height / 2));
            if let Some(center_y) = center_y {
                let point = POINT {
                    x: rect.left + center_x as i32,
                    y: rect.top + center_y as i32,
                };
                let mut original_cursor = POINT::default();
                if unsafe { GetCursorPos(&mut original_cursor) } == 0 {
                    Err(format!("GetCursorPos failed: {}", unsafe {
                        GetLastError()
                    }))
                } else if unsafe { SetCursorPos(point.x, point.y) } == 0 {
                    Err(format!("SetCursorPos failed: {}", unsafe {
                        GetLastError()
                    }))
                } else {
                    let _restore_cursor = CursorRestore(original_cursor);
                    let mut cursor_at_point = POINT::default();
                    unsafe { GetCursorPos(&mut cursor_at_point) };
                    let result = if cursor_at_point.x != point.x || cursor_at_point.y != point.y {
                        Err(format!(
                            "Cursor did not reach opaque island pixel ({},{}); actual=({}, {})",
                            point.x, point.y, cursor_at_point.x, cursor_at_point.y
                        ))
                    } else {
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_millis(100);
                        while std::time::Instant::now() < deadline {
                            let mut pending_message = unsafe { std::mem::zeroed() };
                            while unsafe {
                                PeekMessageW(&mut pending_message, null_mut(), 0, 0, PM_REMOVE)
                            } != 0
                            {
                                unsafe {
                                    TranslateMessage(&pending_message);
                                    DispatchMessageW(&pending_message);
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        let foreground_after_hover = unsafe { GetForegroundWindow() };
                        if foreground_after_hover != foreground_before {
                            Err(format!(
                                "Foreground changed during hover: before={:#x}, after={:#x}",
                                foreground_before as usize, foreground_after_hover as usize
                            ))
                        } else {
                            Ok(format!(
                                "Cursor reached opaque island pixel ({},{}); foreground unchanged",
                                point.x, point.y
                            ))
                        }
                    };
                    drop(_restore_cursor);
                    let mut cursor_restored = POINT::default();
                    let restore_result = if unsafe { GetCursorPos(&mut cursor_restored) } == 0 {
                        Err(format!("GetCursorPos after hover failed: {}", unsafe {
                            GetLastError()
                        }))
                    } else if cursor_restored.x != original_cursor.x
                        || cursor_restored.y != original_cursor.y
                    {
                        Err(format!(
                            "Cursor was not restored after hover: expected=({},{}), actual=({}, {})",
                            original_cursor.x,
                            original_cursor.y,
                            cursor_restored.x,
                            cursor_restored.y
                        ))
                    } else {
                        Ok(())
                    };
                    result.and_then(|message| restore_result.map(|()| message))
                }
            } else {
                Err("No opaque island pixel is available for hover verification".into())
            }
        } else {
            Ok(String::new())
        };

        let pointer_result = if verify_pointer {
            let pointer_target =
                pointer_target.expect("pointer verification creates a target HWND");
            let center_x = scan_width / 2;
            let Some(center_y) = (0..scan_height)
                .rev()
                .find(|pixel_y| pixels[(*pixel_y * scan_width + center_x) * 4 + 3] == 0)
            else {
                unsafe {
                    DestroyWindow(hwnd);
                    DestroyWindow(pointer_target);
                    SelectObject(source_dc, previous_bitmap);
                    DeleteObject(bitmap as HGDIOBJ);
                    DeleteDC(source_dc);
                }
                return Err(
                    "No alpha-zero pixel exists at the shell probe's horizontal center".into(),
                );
            };
            let point = POINT {
                x: rect.left + center_x as i32,
                y: rect.top + center_y as i32,
            };
            let underlying_window = unsafe { WindowFromPhysicalPoint(point) };
            let result = if underlying_window != pointer_target {
                Err(format!(
                    "Transparent-pixel hit test returned HWND {:#x}, expected the isolated target {:#x}; no click was sent",
                    underlying_window as usize, pointer_target as usize
                ))
            } else {
                let mut original_cursor = POINT::default();
                if unsafe { GetCursorPos(&mut original_cursor) } == 0 {
                    Err(format!("GetCursorPos failed: {}", unsafe {
                        GetLastError()
                    }))
                } else if unsafe { SetCursorPos(point.x, point.y) } == 0 {
                    Err(format!("SetCursorPos failed: {}", unsafe {
                        GetLastError()
                    }))
                } else {
                    let _restore_cursor = CursorRestore(original_cursor);
                    POINTER_TARGET_CLICK_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
                    let inputs = [
                        INPUT {
                            r#type: INPUT_MOUSE,
                            Anonymous: INPUT_0 {
                                mi: MOUSEINPUT {
                                    dwFlags: MOUSEEVENTF_LEFTDOWN,
                                    ..Default::default()
                                },
                            },
                        },
                        INPUT {
                            r#type: INPUT_MOUSE,
                            Anonymous: INPUT_0 {
                                mi: MOUSEINPUT {
                                    dwFlags: MOUSEEVENTF_LEFTUP,
                                    ..Default::default()
                                },
                            },
                        },
                    ];
                    let sent = unsafe {
                        SendInput(
                            inputs.len() as u32,
                            inputs.as_ptr(),
                            std::mem::size_of::<INPUT>() as i32,
                        )
                    };
                    let result = if sent != inputs.len() as u32 {
                        Err(format!(
                            "SendInput inserted {sent}/{} mouse events; error={}",
                            inputs.len(),
                            unsafe { GetLastError() }
                        ))
                    } else {
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(2);
                        while POINTER_TARGET_CLICK_COUNT.load(std::sync::atomic::Ordering::SeqCst)
                            == 0
                            && std::time::Instant::now() < deadline
                        {
                            let mut pending_message = unsafe { std::mem::zeroed() };
                            while unsafe {
                                PeekMessageW(&mut pending_message, null_mut(), 0, 0, PM_REMOVE)
                            } != 0
                            {
                                unsafe {
                                    TranslateMessage(&pending_message);
                                    DispatchMessageW(&pending_message);
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        let delivered =
                            POINTER_TARGET_CLICK_COUNT.load(std::sync::atomic::Ordering::SeqCst);
                        let foreground_after_click = unsafe { GetForegroundWindow() };
                        if delivered != 1 {
                            Err(format!(
                                "Expected one underlying WM_LBUTTONUP, received {delivered}"
                            ))
                        } else if foreground_after_click != foreground_before {
                            Err(format!(
                                "Foreground changed during click-through: before={:#x}, after={:#x}",
                                foreground_before as usize, foreground_after_click as usize
                            ))
                        } else {
                            Ok(format!(
                                "Pointer click through alpha-zero pixel ({},{}) reached isolated HWND {:#x}; foreground unchanged",
                                point.x, point.y, pointer_target as usize
                            ))
                        }
                    };
                    drop(_restore_cursor);
                    let mut cursor_restored = POINT::default();
                    let restore_result = if unsafe { GetCursorPos(&mut cursor_restored) } == 0 {
                        Err(format!("GetCursorPos after click failed: {}", unsafe {
                            GetLastError()
                        }))
                    } else if cursor_restored.x != original_cursor.x
                        || cursor_restored.y != original_cursor.y
                    {
                        Err(format!(
                            "Cursor was not restored after click: expected=({},{}), actual=({}, {})",
                            original_cursor.x,
                            original_cursor.y,
                            cursor_restored.x,
                            cursor_restored.y
                        ))
                    } else {
                        Ok(())
                    };
                    result.and_then(|message| restore_result.map(|()| message))
                }
            };
            Some(result)
        } else {
            None
        };

        if let Some(Ok(message)) = &pointer_result {
            println!("{message}");
        }
        if let Ok(message) = &hover_result
            && !message.is_empty()
        {
            println!("{message}");
        }
        unsafe {
            DestroyWindow(hwnd);
            if let Some(target) = pointer_target {
                DestroyWindow(target);
            }
            SelectObject(source_dc, previous_bitmap);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(source_dc);
        }
        if let Some(Err(error)) = pointer_result {
            return Err(error.into());
        }
        if let Err(error) = hover_result {
            return Err(error.into());
        }
        if mismatches != 0 {
            return Err(
                format!("WindowFromPoint hit-map mismatched at {mismatches} pixels").into(),
            );
        }
        return Ok(());
    }
    println!(
        "Probe active. Try --capsule, --light, --left, --scale 80..200, --transparency 0..100, --exclude-from-capture, --verify-hitmap, --verify-hover, or --verify-pointer. Ctrl+C exits."
    );

    let mut message = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unsafe {
        SelectObject(source_dc, previous_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(source_dc);
    }
    Ok(())
}
