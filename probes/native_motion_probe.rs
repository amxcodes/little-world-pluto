//! Native egui/wgpu reference-shape and spring-motion prototype.
//!
//! This is a regular desktop window used to test shared Rust rendering and
//! motion. It is not a platform edge/layer-shell implementation.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use pluto::{
    motion::{
        COLLAPSE_SPRING, CONTENT_ENTER_DELAY, CONTENT_ENTER_DURATION, CONTENT_EXIT_DURATION,
        OPEN_SPRING, SpringMotion,
    },
    visual::{
        EXPANDED_HEIGHT, EXPANDED_WIDTH, FOLDED_HEIGHT, FOLDED_STATUS_WIDTH, NOTCH_EAR, ShapeMode,
    },
};
#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const CANVAS_DARK: Color32 = Color32::from_rgb(0x30, 0x3b, 0x3d);
const CANVAS_LIGHT: Color32 = Color32::from_rgb(0xde, 0xe3, 0xe7);
const DARK_SECONDARY: Color32 = Color32::from_rgba_premultiplied(158, 158, 158, 158);
const LIGHT_SURFACE: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf5);
const LIGHT_INK: Color32 = Color32::from_rgb(0x17, 0x17, 0x1a);
const LIGHT_ACCENT: Color32 = Color32::from_rgb(0x4a, 0x5f, 0xe0);
const DARK_ACCENT: Color32 = Color32::from_rgb(0x8e, 0xa2, 0xff);
#[derive(Clone, Copy)]
struct OpacityTween {
    from: f32,
    to: f32,
    started_at: Instant,
    delay: Duration,
    duration: Duration,
    ease_cubic: bool,
}

impl OpacityTween {
    fn new(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            started_at: now,
            delay: Duration::ZERO,
            duration: Duration::ZERO,
            ease_cubic: false,
        }
    }

    fn retarget(
        &mut self,
        target: f32,
        delay: Duration,
        duration: Duration,
        ease_cubic: bool,
        now: Instant,
    ) {
        self.from = self.value(now);
        self.to = target;
        self.started_at = now;
        self.delay = delay;
        self.duration = duration;
        self.ease_cubic = ease_cubic;
    }

    fn value(self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed <= self.delay {
            return self.from;
        }
        if self.duration.is_zero() || elapsed >= self.delay + self.duration {
            return self.to;
        }

        let progress = (elapsed - self.delay).as_secs_f32() / self.duration.as_secs_f32();
        let progress = progress.clamp(0.0, 1.0);
        let eased = if self.ease_cubic {
            reference_content_ease(progress)
        } else {
            progress
        };
        self.from + (self.to - self.from) * eased
    }

    fn is_active(self, now: Instant) -> bool {
        self.value(now) != self.to
    }
}

#[derive(Clone, Copy)]
struct ContentMotion {
    status: OpacityTween,
    expanded: OpacityTween,
}

impl ContentMotion {
    fn new(now: Instant) -> Self {
        Self {
            status: OpacityTween::new(1.0, now),
            expanded: OpacityTween::new(0.0, now),
        }
    }

    fn retarget(&mut self, expanded: bool, now: Instant) {
        self.status.retarget(
            if expanded { 0.0 } else { 1.0 },
            if expanded {
                Duration::ZERO
            } else {
                CONTENT_ENTER_DELAY
            },
            if expanded {
                CONTENT_EXIT_DURATION
            } else {
                CONTENT_ENTER_DURATION
            },
            !expanded,
            now,
        );
        self.expanded.retarget(
            if expanded { 1.0 } else { 0.0 },
            if expanded {
                CONTENT_ENTER_DELAY
            } else {
                Duration::ZERO
            },
            if expanded {
                CONTENT_ENTER_DURATION
            } else {
                CONTENT_EXIT_DURATION
            },
            expanded,
            now,
        );
    }

    fn status_opacity(self, now: Instant) -> f32 {
        self.status.value(now)
    }

    fn expanded_opacity(self, now: Instant) -> f32 {
        self.expanded.value(now)
    }

    fn is_active(self, now: Instant) -> bool {
        self.status.is_active(now) || self.expanded.is_active(now)
    }
}

fn reference_content_ease(progress: f32) -> f32 {
    // Cubic-bezier(.2, .8, .2, 1), recovered from the reference content enter.
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..12 {
        let t = (low + high) * 0.5;
        let x = cubic_bezier_coordinate(t, 0.2, 0.2);
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    cubic_bezier_coordinate((low + high) * 0.5, 0.8, 1.0)
}

fn cubic_bezier_coordinate(t: f32, first_control: f32, second_control: f32) -> f32 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * t * first_control + 3.0 * inverse * t * t * second_control + t * t * t
}

#[derive(Clone, Copy)]
struct Palette {
    surface: Color32,
    ink: Color32,
    secondary: Color32,
    accent: Color32,
    dark_theme: bool,
}

struct MotionProbe {
    edge_mode: bool,
    edge_auto_motion: bool,
    edge_auto_opened: bool,
    edge_auto_closing: bool,
    edge_auto_completed: bool,
    edge_auto_started_at: Instant,
    edge_auto_close_at: Option<Instant>,
    edge_geometry_logged: bool,
    edge_passthrough_configured: bool,
    edge_setup_error_logged: bool,
    shape: ShapeMode,
    expanded: bool,
    light_theme: bool,
    reduced_motion: bool,
    width: SpringMotion,
    height: SpringMotion,
    radius: SpringMotion,
    content: ContentMotion,
    previous_frame: Instant,
    last_frame_ms: f32,
    notch_ear_texture: Option<(f32, egui::TextureHandle)>,
}

impl Default for MotionProbe {
    fn default() -> Self {
        Self {
            edge_mode: false,
            edge_auto_motion: false,
            edge_auto_opened: false,
            edge_auto_closing: false,
            edge_auto_completed: false,
            edge_auto_started_at: Instant::now(),
            edge_auto_close_at: None,
            edge_geometry_logged: false,
            edge_passthrough_configured: false,
            edge_setup_error_logged: false,
            shape: ShapeMode::Notch,
            expanded: false,
            light_theme: false,
            reduced_motion: false,
            width: SpringMotion::new(f64::from(FOLDED_STATUS_WIDTH), COLLAPSE_SPRING),
            height: SpringMotion::new(f64::from(FOLDED_HEIGHT), COLLAPSE_SPRING),
            radius: SpringMotion::new(15.0, COLLAPSE_SPRING),
            content: ContentMotion::new(Instant::now()),
            previous_frame: Instant::now(),
            last_frame_ms: 0.0,
            notch_ear_texture: None,
        }
    }
}

impl MotionProbe {
    fn notch_ear_texture(&mut self, ctx: &egui::Context) -> egui::TextureId {
        let pixels_per_point = ctx.pixels_per_point();
        if self
            .notch_ear_texture
            .as_ref()
            .is_none_or(|(scale, _)| (scale - pixels_per_point).abs() > f32::EPSILON)
        {
            let image = notch_ear_image(pixels_per_point);
            let handle =
                ctx.load_texture("notch-ear-coverage", image, egui::TextureOptions::LINEAR);
            self.notch_ear_texture = Some((pixels_per_point, handle));
        }
        self.notch_ear_texture.as_ref().unwrap().1.id()
    }

    fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
        let now = Instant::now();
        let (spring, width, height, radius) = if self.expanded {
            (
                OPEN_SPRING,
                EXPANDED_WIDTH,
                EXPANDED_HEIGHT,
                ShapeMode::Notch.lower_radius(EXPANDED_HEIGHT, true),
            )
        } else {
            (
                COLLAPSE_SPRING,
                FOLDED_STATUS_WIDTH,
                FOLDED_HEIGHT,
                ShapeMode::Notch.lower_radius(FOLDED_HEIGHT, false),
            )
        };

        self.width.retarget(f64::from(width), spring);
        self.height.retarget(f64::from(height), spring);
        self.radius.retarget(f64::from(radius), spring);
        self.content.retarget(self.expanded, now);

        if self.reduced_motion {
            self.width.snap_to(f64::from(width));
            self.height.snap_to(f64::from(height));
            self.radius.snap_to(f64::from(radius));
            self.content = ContentMotion {
                status: OpacityTween::new(if self.expanded { 0.0 } else { 1.0 }, now),
                expanded: OpacityTween::new(if self.expanded { 1.0 } else { 0.0 }, now),
            };
        }
    }

    fn step(&mut self, elapsed: Duration, now: Instant) -> bool {
        self.last_frame_ms = elapsed.as_secs_f32() * 1_000.0;
        let width = self.width.advance(elapsed);
        let height = self.height.advance(elapsed);
        let radius = self.radius.advance(elapsed);
        width || height || radius || self.content.is_active(now)
    }
}

impl eframe::App for MotionProbe {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.edge_mode {
            Color32::TRANSPARENT.to_normalized_gamma_f32()
        } else {
            visuals.window_fill().to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        if self.edge_auto_motion
            && !self.edge_auto_opened
            && now.saturating_duration_since(self.edge_auto_started_at)
                >= Duration::from_millis(2_000)
        {
            println!("Edge motion: opening");
            self.toggle_expanded();
            self.edge_auto_opened = true;
        }
        let elapsed = now.saturating_duration_since(self.previous_frame);
        self.previous_frame = now;

        if ui.ctx().input(|input| input.key_pressed(egui::Key::Space)) {
            self.toggle_expanded();
        }
        let moving = self.step(elapsed, now);
        if self.edge_auto_motion && self.edge_auto_opened && self.expanded && !moving {
            let close_at = self
                .edge_auto_close_at
                .get_or_insert(now + Duration::from_millis(1_800));
            if now >= *close_at {
                println!("Edge motion: reversing to folded");
                self.toggle_expanded();
                self.edge_auto_closing = true;
                self.edge_auto_close_at = None;
            }
        } else if self.edge_auto_motion && self.edge_auto_closing && !self.expanded && !moving {
            println!("Edge motion: settled folded");
            self.edge_auto_completed = true;
        }
        if moving || (self.edge_auto_motion && !self.edge_auto_completed) {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        let canvas = if self.light_theme {
            CANVAS_LIGHT
        } else {
            CANVAS_DARK
        };
        let palette = if self.light_theme {
            Palette {
                surface: LIGHT_SURFACE,
                ink: LIGHT_INK,
                secondary: Color32::from_rgb(0x6a, 0x6a, 0x72),
                accent: LIGHT_ACCENT,
                dark_theme: false,
            }
        } else {
            Palette {
                surface: Color32::BLACK,
                ink: Color32::WHITE,
                secondary: DARK_SECONDARY,
                accent: DARK_ACCENT,
                dark_theme: true,
            }
        };

        let panel_frame = if self.edge_mode {
            egui::Frame::NONE
        } else {
            egui::Frame::default()
                .fill(Color32::from_rgb(0x12, 0x13, 0x16))
                .inner_margin(20.0)
        };
        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ui, |ui| {
                if !self.edge_mode {
                    ui.horizontal(|ui| {
                        ui.heading("Island · Rust UI host");
                        ui.add_space(12.0);
                        if ui
                            .button(if self.expanded {
                                "Fold  ·  Space"
                            } else {
                                "Expand  ·  Space"
                            })
                            .clicked()
                        {
                            self.toggle_expanded();
                        }
                        ui.selectable_value(&mut self.shape, ShapeMode::Notch, "Notch");
                        ui.selectable_value(&mut self.shape, ShapeMode::Capsule, "Capsule");
                        ui.checkbox(&mut self.light_theme, "Light");
                        ui.checkbox(&mut self.reduced_motion, "Reduced motion");
                    });

                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Reference motion fixture · synthetic content · no Codex connection",
                        )
                        .color(palette.secondary),
                    );
                    ui.add_space(14.0);
                }

                let canvas_height = if self.edge_mode {
                    ui.available_height()
                } else {
                    450.0
                };
                let (canvas_rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), canvas_height),
                    Sense::hover(),
                );
                if self.edge_mode && !self.edge_geometry_logged {
                    let body_width = self.width.value();
                    let body_height = self.height.value();
                    println!("Edge surface geometry: canvas={canvas_rect:?}, folded_body={body_width:.1}x{body_height:.1}pt");
                    self.edge_geometry_logged = true;
                }
                let painter = ui.painter().with_clip_rect(canvas_rect);
                if !self.edge_mode {
                    painter.rect_filled(canvas_rect, 12.0, canvas);
                    painter.text(
                        canvas_rect.left_top() + Vec2::splat(14.0),
                        Align2::LEFT_TOP,
                        "DESKTOP SURFACE PREVIEW",
                        FontId::proportional(10.0),
                        palette.secondary,
                    );
                }

                let body_width = self.width.value() as f32;
                let body_height = self.height.value() as f32;
                let total_width = self.shape.surface_width(body_width);
                let left = canvas_rect.center().x - total_width / 2.0;
                let body_left = left + self.shape.body_inset();
                let body_top = canvas_rect.top() + self.shape.top_gap();
                let body = Rect::from_min_size(
                    Pos2::new(body_left, body_top),
                    Vec2::new(body_width, body_height),
                );
                let radius = (self.radius.value() as f32).clamp(0.0, body_height / 2.0);

                #[cfg(windows)]
                if self.edge_mode && !self.edge_passthrough_configured {
                    // eframe applies its initial viewport commands after ui().
                    // Wait until those have settled before the native policy.
                    if self.edge_auto_started_at.elapsed() < Duration::from_millis(500) {
                        ui.ctx().request_repaint_after(Duration::from_millis(100));
                    } else {
                        match configure_edge_window_passthrough(_frame) {
                            Ok(styles) => {
                                println!("Edge renderer remains fully click-through: {styles}");
                                self.edge_passthrough_configured = true;
                                self.edge_setup_error_logged = false;
                            }
                            Err(error) => {
                                if !self.edge_setup_error_logged {
                                    eprintln!("Could not verify edge click-through: {error}");
                                    self.edge_setup_error_logged = true;
                                }
                                ui.ctx().request_repaint_after(Duration::from_millis(100));
                            }
                        }
                    }
                }

                if self.shape == ShapeMode::Notch {
                    let ear_texture = self.notch_ear_texture(ui.ctx());
                    paint_notch_ears(&painter, left, body.right(), body_top, palette.surface, ear_texture);
                }
                paint_body(&painter, body, radius, self.shape, palette.surface);
                paint_status_row(
                    &painter,
                    body,
                    palette,
                    self.content.status_opacity(now),
                );
                paint_expanded_content(
                    &painter,
                    body,
                    palette,
                    self.content.expanded_opacity(now),
                );

                if !self.edge_mode {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "State: {}",
                            if self.expanded { "expanded" } else { "folded" }
                        ));
                        ui.separator();
                        ui.label(format!("Frame: {:.1} ms", self.last_frame_ms));
                        ui.separator();
                        ui.label(if moving {
                            "Repainting while a transition is active"
                        } else {
                            "Idle: event-driven repaint"
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "This window validates shared rendering and motion only. Native edge placement, transparent input regions, and Wayland layer-shell are tested separately.",
                        )
                        .small()
                        .color(palette.secondary),
                    );
                }
            });
    }
}

#[cfg(windows)]
fn configure_edge_window_passthrough(frame: &eframe::Frame) -> Result<String, String> {
    use windows_sys::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
        },
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE,
            SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_EX_CLIENTEDGE,
            WS_EX_DLGMODALFRAME, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_STATICEDGE,
            WS_EX_TRANSPARENT, WS_EX_WINDOWEDGE, WS_OVERLAPPEDWINDOW, WS_POPUP,
        },
    };

    let window_handle = frame
        .window_handle()
        .map_err(|error| format!("could not read the renderer window handle: {error}"))?;
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return Err("the renderer did not create a Win32 HWND".to_owned());
    };
    let hwnd = handle.hwnd.get() as HWND;
    // The pill owns its contour. A DWM frame adds a rectangular border/shadow
    // around the otherwise transparent host, independently of our paint code.
    for (attribute, value) in [
        (DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED as u32),
        (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND as u32),
        (DWMWA_BORDER_COLOR, 0xFFFF_FFFE),
    ] {
        let result = unsafe {
            DwmSetWindowAttribute(hwnd, attribute as u32, (&value as *const u32).cast(), 4)
        };
        if result < 0 {
            eprintln!("DWM overlay attribute {attribute} unavailable: 0x{result:X}");
        }
    }
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let requested = (current | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT)
        & !(WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            ((style & !WS_OVERLAPPEDWINDOW) | WS_POPUP) as isize,
        );
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, requested as isize);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOACTIVATE
                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        );
        windows_sys::Win32::Graphics::Gdi::SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
    }

    let actual = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let required = WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
    if actual & required != required {
        return Err(format!(
            "unexpected HWND styles for smooth click-through rendering (exstyle=0x{actual:X})"
        ));
    }
    Ok(format!(
        "hwnd=0x{:X}, exstyle=0x{actual:X}, layered=true, no-activate=true, WS_EX_TRANSPARENT=true, custom-shape-region=false",
        hwnd as usize
    ))
}
fn paint_notch_ears(
    painter: &egui::Painter,
    outer_left: f32,
    body_right: f32,
    body_top: f32,
    surface: Color32,
    texture: egui::TextureId,
) {
    let overlap = painter.ctx().pixels_per_point().recip();
    let left = Rect::from_min_max(
        Pos2::new(outer_left, body_top),
        Pos2::new(outer_left + NOTCH_EAR + overlap, body_top + NOTCH_EAR),
    );
    let right = Rect::from_min_max(
        Pos2::new(body_right - overlap, body_top),
        Pos2::new(body_right + NOTCH_EAR, body_top + NOTCH_EAR),
    );
    painter.image(
        texture,
        left,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        surface,
    );
    painter.image(
        texture,
        right,
        Rect::from_min_max(Pos2::new(1.0, 0.0), Pos2::new(0.0, 1.0)),
        surface,
    );
}

fn notch_ear_image(pixels_per_point: f32) -> egui::ColorImage {
    // The reference uses a CSS radial gradient: transparent through 13.5 px,
    // island color at 14 px. Integrate coverage within each device pixel so
    // the thin transition stays smooth even at non-integral display scales.
    const SAMPLES: usize = 8;
    const FADE_START: f32 = 13.5;
    let width = ((NOTCH_EAR * pixels_per_point).ceil() as usize) + 1;
    let height = (NOTCH_EAR * pixels_per_point).ceil() as usize;
    let logical_width = NOTCH_EAR + pixels_per_point.recip();
    let pixels = (0..height)
        .flat_map(|row| {
            (0..width).map(move |column| {
                let mut coverage = 0.0;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let x = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32)
                            * logical_width
                            / width as f32;
                        let y = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * NOTCH_EAR
                            / height as f32;
                        let alpha = if x >= NOTCH_EAR {
                            1.0
                        } else {
                            (((x * x + (y - NOTCH_EAR).powi(2)).sqrt() - FADE_START)
                                / (NOTCH_EAR - FADE_START))
                                .clamp(0.0, 1.0)
                        };
                        coverage += alpha;
                    }
                }
                let alpha = (coverage * 255.0 / (SAMPLES * SAMPLES) as f32).round() as u8;
                Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
            })
        })
        .collect();
    egui::ColorImage::new([width, height], pixels)
}

fn paint_body(
    painter: &egui::Painter,
    body: Rect,
    radius: f32,
    shape: ShapeMode,
    surface: Color32,
) {
    const ARC_SEGMENTS: usize = 20;
    let radius = radius
        .clamp(0.0, body.height() * 0.5)
        .min(body.width() * 0.5);
    let mut points = Vec::with_capacity(ARC_SEGMENTS * 4 + 4);

    if shape == ShapeMode::Capsule {
        points.push(Pos2::new(body.left() + radius, body.top()));
        points.push(Pos2::new(body.right() - radius, body.top()));
        append_arc(
            &mut points,
            Pos2::new(body.right() - radius, body.top() + radius),
            radius,
            -std::f32::consts::FRAC_PI_2,
            0.0,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut points,
            Pos2::new(body.right() - radius, body.bottom() - radius),
            radius,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut points,
            Pos2::new(body.left() + radius, body.bottom() - radius),
            radius,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut points,
            Pos2::new(body.left() + radius, body.top() + radius),
            radius,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
            ARC_SEGMENTS,
        );
    } else {
        points.push(body.left_top());
        points.push(body.right_top());
        points.push(Pos2::new(body.right(), body.bottom() - radius));
        append_arc(
            &mut points,
            Pos2::new(body.right() - radius, body.bottom() - radius),
            radius,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ARC_SEGMENTS,
        );
        append_arc(
            &mut points,
            Pos2::new(body.left() + radius, body.bottom() - radius),
            radius,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            ARC_SEGMENTS,
        );
        points.push(body.left_top());
    }

    painter.add(egui::Shape::convex_polygon(points, surface, Stroke::NONE));
}

fn append_arc(
    points: &mut Vec<Pos2>,
    center: Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    segments: usize,
) {
    for step in 1..=segments {
        let progress = step as f32 / segments as f32;
        let angle = start_angle + (end_angle - start_angle) * progress;
        points.push(center + Vec2::new(angle.cos(), angle.sin()) * radius);
    }
}

fn paint_status_row(painter: &egui::Painter, body: Rect, palette: Palette, opacity: f32) {
    if opacity <= 0.01 {
        return;
    }
    let ink = palette.ink.gamma_multiply(opacity);
    let secondary = palette.secondary.gamma_multiply(opacity);
    let row_width = 126.0;
    let start_x = body.center().x - row_width / 2.0;
    let text_y = body.center().y - 8.0;
    let font = FontId::proportional(12.0);
    painter.text(
        Pos2::new(start_x, text_y),
        Align2::LEFT_TOP,
        "7:04",
        font.clone(),
        ink,
    );

    let mascot_x = body.left() + 10.0;
    let mascot_y = body.center().y;
    painter.circle_filled(Pos2::new(mascot_x, mascot_y), 4.2, ink);
    painter.circle_filled(Pos2::new(mascot_x + 4.0, mascot_y - 2.0), 3.7, ink);
    painter.circle_filled(Pos2::new(mascot_x + 7.0, mascot_y + 0.2), 2.8, ink);
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(mascot_x - 3.0, mascot_y),
            Pos2::new(mascot_x + 7.0, mascot_y + 3.2),
        ),
        1.5,
        ink,
    );

    let battery = Rect::from_min_size(
        Pos2::new(start_x + 38.0, text_y + 2.0),
        Vec2::new(14.0, 10.0),
    );
    painter.rect_stroke(battery, 2.0, Stroke::new(1.2, ink), StrokeKind::Inside);
    painter.rect_filled(
        Rect::from_min_size(battery.min + Vec2::new(2.0, 2.0), Vec2::new(8.0, 6.0)),
        1.0,
        Color32::from_rgb(0x35, 0xd6, 0x9a),
    );
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(battery.right(), battery.center().y - 2.0),
            Vec2::new(2.0, 4.0),
        ),
        0.5,
        ink,
    );
    painter.text(
        Pos2::new(start_x + 56.0, text_y),
        Align2::LEFT_TOP,
        "100%",
        font.clone(),
        ink,
    );

    let weather_x = start_x + 101.0;
    painter.circle_filled(Pos2::new(weather_x + 3.0, text_y + 7.0), 3.0, secondary);
    painter.circle_filled(Pos2::new(weather_x + 7.0, text_y + 6.0), 4.0, secondary);
    painter.text(
        Pos2::new(weather_x + 12.0, text_y),
        Align2::LEFT_TOP,
        "29°",
        font,
        ink,
    );
}

fn paint_expanded_content(painter: &egui::Painter, body: Rect, palette: Palette, opacity: f32) {
    if opacity <= 0.01 {
        return;
    }
    let ink = palette.ink.gamma_multiply(opacity);
    let secondary = palette.secondary.gamma_multiply(opacity);
    let accent = palette.accent.gamma_multiply(opacity);
    let left = body.left() + 24.0;
    let top = body.top() + 26.0;
    painter.text(
        Pos2::new(left, top),
        Align2::LEFT_TOP,
        "Expanded renderer fixture",
        FontId::proportional(18.0),
        ink,
    );
    painter.text(
        Pos2::new(left, top + 28.0),
        Align2::LEFT_TOP,
        "680 × 336 logical points  ·  synthetic content",
        FontId::proportional(13.0),
        secondary,
    );

    let card_y = top + 68.0;
    let gap = 12.0;
    let card_width = (body.width() - 48.0 - 2.0 * gap) / 3.0;
    let card_height = (body.height() - 112.0).max(62.0);
    for (index, (title, detail)) in [
        ("Content region 1", "Placeholder fixture"),
        ("Content region 2", "No runtime data"),
        ("Content region 3", "Not connected"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = left + index as f32 * (card_width + gap);
        let card = Rect::from_min_size(Pos2::new(x, card_y), Vec2::new(card_width, card_height));
        let card_surface = if palette.dark_theme {
            Color32::from_rgb(0x19, 0x1a, 0x1f)
        } else {
            Color32::from_rgb(0xe8, 0xe9, 0xed)
        }
        .gamma_multiply(opacity);
        painter.rect_filled(card, 10.0, card_surface);
        painter.circle_filled(
            Pos2::new(card.left() + 14.0, card.top() + 17.0),
            4.0,
            accent,
        );
        painter.text(
            Pos2::new(card.left() + 26.0, card.top() + 9.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(13.0),
            ink,
        );
        painter.text(
            Pos2::new(card.left() + 12.0, card.top() + 36.0),
            Align2::LEFT_TOP,
            detail,
            FontId::proportional(11.0),
            secondary,
        );
        painter.text(
            Pos2::new(card.left() + 12.0, card.bottom() - 22.0),
            Align2::LEFT_TOP,
            "Fixture",
            FontId::proportional(11.0),
            accent,
        );
    }
}

#[cfg(not(any(
    feature = "ui-host-probe",
    feature = "ui-host-profile-wgpu",
    feature = "ui-host-profile-glow"
)))]
compile_error!("select ui-host-probe or exactly one ui-host-profile-* renderer feature");

#[cfg(windows)]
fn primary_monitor_edge_position() -> egui::Pos2 {
    use windows_sys::Win32::{
        UI::HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
            SetProcessDpiAwarenessContext,
        },
        UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN},
    };

    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let dpi = unsafe { GetDpiForSystem() }.max(96) as f32;
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32 * 96.0 / dpi;
    egui::pos2(((screen_width - 760.0) / 2.0).max(0.0), 0.0)
}

#[cfg(not(windows))]
fn primary_monitor_edge_position() -> egui::Pos2 {
    egui::pos2(0.0, 0.0)
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let small_shell = args.iter().any(|arg| arg == "--small-shell");
    let edge_mode = small_shell || args.iter().any(|arg| arg == "--edge");
    let edge_auto_motion = args.iter().any(|arg| arg == "--auto-motion");
    let light_theme = args.iter().any(|arg| arg == "--light");
    let shape = if args.iter().any(|arg| arg == "--capsule") {
        ShapeMode::Capsule
    } else {
        ShapeMode::Notch
    };
    if edge_mode {
        println!("Edge diagnostic args={args:?}; auto-motion={edge_auto_motion}");
    }

    #[cfg(feature = "ui-host-probe")]
    let renderer = if args.iter().any(|arg| arg == "--glow") {
        eframe::Renderer::Glow
    } else {
        eframe::Renderer::Wgpu
    };

    #[cfg(all(
        feature = "ui-host-profile-wgpu",
        not(any(feature = "ui-host-probe", feature = "ui-host-profile-glow"))
    ))]
    let renderer = eframe::Renderer::Wgpu;

    #[cfg(all(
        feature = "ui-host-profile-glow",
        not(any(feature = "ui-host-probe", feature = "ui-host-profile-wgpu"))
    ))]
    let renderer = eframe::Renderer::Glow;

    #[cfg(all(
        feature = "ui-host-profile-wgpu",
        feature = "ui-host-profile-glow",
        not(feature = "ui-host-probe")
    ))]
    let renderer = if args.iter().any(|arg| arg == "--glow") {
        eframe::Renderer::Glow
    } else {
        eframe::Renderer::Wgpu
    };

    #[cfg(all(
        windows,
        any(feature = "ui-host-probe", feature = "ui-host-profile-glow")
    ))]
    if edge_mode && renderer == eframe::Renderer::Glow {
        return Err(eframe::Error::AppCreation(
            "Windows desktop overlays require the WGPU DirectComposition path; use Glow only for the regular preview window.".into(),
        ));
    }

    let viewport = if small_shell {
        egui::ViewportBuilder::default()
            .with_inner_size([230.0, 30.0])
            .with_min_inner_size([230.0, 30.0])
            .with_max_inner_size([230.0, 30.0])
            .with_position(primary_monitor_edge_position())
            .with_decorations(false)
            .with_resizable(false)
            .with_has_shadow(false)
            .with_transparent(true)
            .with_active(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
    } else if edge_mode {
        egui::ViewportBuilder::default()
            .with_inner_size([760.0, 480.0])
            .with_min_inner_size([760.0, 480.0])
            .with_max_inner_size([760.0, 480.0])
            .with_position(primary_monitor_edge_position())
            .with_decorations(false)
            .with_resizable(false)
            .with_has_shadow(false)
            .with_transparent(true)
            .with_active(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 720.0])
            .with_min_inner_size([900.0, 660.0])
    };
    let options = eframe::NativeOptions {
        renderer,
        viewport,
        ..Default::default()
    };
    #[cfg(all(
        windows,
        any(feature = "ui-host-probe", feature = "ui-host-profile-wgpu")
    ))]
    let options = {
        let mut options = options;
        if edge_mode && renderer == eframe::Renderer::Wgpu {
            if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) =
                &mut options.wgpu_options.wgpu_setup
            {
                // The default HWND swapchain is opaque on Windows. Request
                // the alpha-capable DirectComposition visual explicitly.
                setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12;
                setup
                    .instance_descriptor
                    .backend_options
                    .dx12
                    .presentation_system = eframe::wgpu::Dx12SwapchainKind::DxgiFromVisual;
            }
            println!("Desktop compositor: DX12 DirectComposition visual (per-pixel alpha)");
        }
        options
    };

    eframe::run_native(
        if edge_mode {
            "Island Rust UI Host — Phase 0 (transparent edge diagnostic)"
        } else {
            "Island Rust UI Host — Phase 0"
        },
        options,
        Box::new(move |context| {
            context.egui_ctx.set_visuals(egui::Visuals::dark());
            let app = MotionProbe {
                edge_mode,
                edge_auto_motion,
                edge_auto_started_at: Instant::now(),
                light_theme,
                shape,
                ..MotionProbe::default()
            };
            Ok(Box::new(app))
        }),
    )
}
