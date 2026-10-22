//! Shared layout targets recovered from the reference UI.
//!
//! These are logical points. The renderer/window host applies the current
//! device scale exactly once.

pub const IDLE_MIN_WIDTH: f32 = 150.0;
pub const FOLDED_STATUS_WIDTH: f32 = 202.0;
/// Screenshot-calibrated status span; kept separate from the extracted 202 px source target.
pub const CALIBRATED_STATUS_WIDTH: f64 = 193.6;
pub const FOLDED_HEIGHT: f32 = 30.0;
pub const FOLDED_HOST_HEIGHT: f64 = 33.6;
pub const FOLDED_HOST_TAIL: f64 = FOLDED_HOST_HEIGHT - FOLDED_HEIGHT as f64;
pub const NOTCH_EAR: f32 = 14.0;
pub const CAPSULE_GAP: f32 = 8.0;
pub const MEDIA_WIDTH: f32 = 268.0;
pub const MEDIA_HEIGHT: f32 = 32.0;
pub const DOWNLOADS_WIDTH: f32 = 320.0;
pub const EXPANDED_WIDTH: f32 = 680.0;
pub const EXPANDED_HEIGHT: f32 = 336.0;
pub const CHAT_WIDTH: f32 = 680.0;
pub const CHAT_HEIGHT: f32 = 420.0;
pub const COMMAND_WIDTH: f32 = 640.0;
pub const DRAG_TARGET_WIDTH: f32 = 460.0;
pub const DRAG_TARGET_WIDE_WIDTH: f32 = 620.0;
pub const DRAG_TARGET_HEIGHT: f32 = 128.0;

/// Round a physical width to the display's pixel parity so its integer origin
/// can remain exactly centered for every frame of a resize.
pub fn centered_pixel_width(pixels: f64, screen_width: i32) -> u32 {
    let parity = screen_width.rem_euclid(2) as u32;
    (((pixels - f64::from(parity)) / 2.0).round().max(1.0) as u32) * 2 + parity
}

/// Place a surface on an exact pixel center. Widths must share parity; the
/// assertion prevents an accidental half-pixel center from entering a frame.
pub fn centered_pixel_origin(container_width: i32, content_width: i32) -> i32 {
    debug_assert_eq!(
        container_width.rem_euclid(2),
        content_width.rem_euclid(2),
        "centered surfaces must use matching physical-pixel parity"
    );
    (container_width - content_width).div_euclid(2)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapeMode {
    #[default]
    Notch,
    Capsule,
}

impl ShapeMode {
    pub const fn folded_body_width(self) -> f64 {
        match self {
            Self::Notch => CALIBRATED_STATUS_WIDTH,
            Self::Capsule => FOLDED_STATUS_WIDTH as f64,
        }
    }

    pub const fn expanded_body_width(self) -> f64 {
        match self {
            Self::Notch => EXPANDED_WIDTH as f64,
            Self::Capsule => EXPANDED_WIDTH as f64,
        }
    }

    pub const fn surface_width(self, body_width: f32) -> f32 {
        body_width
    }

    pub const fn body_inset(self) -> f32 {
        0.0
    }

    pub const fn top_gap(self) -> f32 {
        match self {
            Self::Notch => 0.0,
            Self::Capsule => CAPSULE_GAP,
        }
    }

    pub fn lower_radius(self, height: f32, expanded: bool) -> f32 {
        let source_limit = if expanded { 32.0 } else { 26.0 };
        (height / 2.0).min(source_limit)
    }
}

/// One measured shell contour used by both the layered-window alpha and hit map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    shape: ShapeMode,
    body_width: f64,
    body_height: f64,
    scale: f64,
}

impl ShellLayout {
    pub fn new(shape: ShapeMode, body_width: f64, scale: f64) -> Option<Self> {
        Self::with_body_height(shape, body_width, f64::from(FOLDED_HEIGHT), scale)
    }

    pub fn with_body_height(
        shape: ShapeMode,
        body_width: f64,
        body_height: f64,
        scale: f64,
    ) -> Option<Self> {
        if !body_width.is_finite()
            || body_width < f64::from(IDLE_MIN_WIDTH)
            || !body_height.is_finite()
            || body_height < f64::from(FOLDED_HEIGHT)
            || !scale.is_finite()
            || scale <= 0.0
        {
            return None;
        }
        Some(Self {
            shape,
            body_width,
            body_height,
            scale,
        })
    }

    pub fn logical_width(self) -> f64 {
        self.body_width
    }

    pub fn physical_size(self) -> (u32, u32) {
        (
            (self.logical_width() * self.scale).round() as u32,
            ((f64::from(self.shape.top_gap()) + self.body_height + FOLDED_HOST_TAIL) * self.scale)
                .round() as u32,
        )
    }

    /// Shape coverage at one logical sample point. Pixel antialiasing is
    /// produced by `pixel_alpha`'s bounded supersampling; a second distance
    /// ramp here would blur the edge twice and leak alpha into the clear gap.
    pub fn coverage(self, x: f64, y: f64) -> f64 {
        if !x.is_finite() || !y.is_finite() {
            return 0.0;
        }

        let y = y - f64::from(self.shape.top_gap());
        let height = self.body_height;
        let inset = f64::from(self.shape.body_inset());
        let radius = f64::from(
            self.shape
                .lower_radius(height as f32, height > f64::from(FOLDED_HEIGHT)),
        );
        let round_top = self.shape == ShapeMode::Capsule;
        let distance = round_rect_signed_distance(
            (x, y),
            (inset, 0.0, self.body_width, height),
            radius,
            round_top,
        );
        f64::from(distance <= 0.0)
    }

    /// Antialiased alpha byte. The same byte must drive rendering and hit testing.
    pub fn pixel_alpha(self, pixel_x: u32, pixel_y: u32, samples_per_axis: usize) -> u8 {
        if samples_per_axis == 0 {
            return 0;
        }

        let pixel_left = f64::from(pixel_x) / self.scale;
        let top_gap = f64::from(self.shape.top_gap());
        let pixel_top = f64::from(pixel_y) / self.scale - top_gap;
        let pixel_right = (f64::from(pixel_x) + 1.0) / self.scale;
        let pixel_bottom = (f64::from(pixel_y) + 1.0) / self.scale - top_gap;
        let body_left = f64::from(self.shape.body_inset());
        let body_right = body_left + self.body_width;
        let body_bottom = self.body_height;
        let radius = f64::from(self.shape.lower_radius(
            self.body_height as f32,
            self.body_height > f64::from(FOLDED_HEIGHT),
        ));

        // Most of a shell frame is fully opaque. Prove the complete physical
        // pixel lies inside the rounded body before doing supersampled curve
        // work; only the narrow antialiased boundary needs the expensive path.
        let within_body = pixel_left >= body_left
            && pixel_right <= body_right
            && pixel_top >= 0.0
            && pixel_bottom <= body_bottom;
        let clear_of_rounded_corners = match self.shape {
            ShapeMode::Notch => {
                pixel_bottom <= body_bottom - radius
                    || (pixel_left >= body_left + radius && pixel_right <= body_right - radius)
            }
            ShapeMode::Capsule => {
                (pixel_top >= radius && pixel_bottom <= body_bottom - radius)
                    || (pixel_left >= body_left + radius && pixel_right <= body_right - radius)
            }
        };
        if within_body && clear_of_rounded_corners {
            return u8::MAX;
        }

        let sample_count = (samples_per_axis * samples_per_axis) as f64;
        let mut coverage = 0.0;
        for sample_y in 0..samples_per_axis {
            for sample_x in 0..samples_per_axis {
                let x = (f64::from(pixel_x) + (sample_x as f64 + 0.5) / samples_per_axis as f64)
                    / self.scale;
                let y = (f64::from(pixel_y) + (sample_y as f64 + 0.5) / samples_per_axis as f64)
                    / self.scale;
                coverage += self.coverage(x, y);
            }
        }
        (coverage / sample_count * 255.0).round() as u8
    }

    /// Fill the rendered/hit-test alpha plane while evaluating only contour
    /// pixels. The broad interior is provably opaque and is filled by row spans.
    pub fn fill_alpha_plane(self, alpha: &mut [u8], samples_per_axis: usize) -> bool {
        let (width, height) = self.physical_size();
        let Some(expected_len) = (width as usize).checked_mul(height as usize) else {
            return false;
        };
        if samples_per_axis == 0 || alpha.len() != expected_len {
            return false;
        }

        let body_left = f64::from(self.shape.body_inset());
        let body_right = body_left + self.body_width;
        let body_bottom = self.body_height;
        let radius = f64::from(self.shape.lower_radius(
            self.body_height as f32,
            self.body_height > f64::from(FOLDED_HEIGHT),
        ));
        alpha.fill(0);

        for y in 0..height {
            let top_gap = f64::from(self.shape.top_gap());
            let pixel_top = f64::from(y) / self.scale - top_gap;
            let pixel_bottom = (f64::from(y) + 1.0) / self.scale - top_gap;
            if pixel_top >= body_bottom || pixel_bottom <= 0.0 {
                continue;
            }

            let fully_inside_y = pixel_bottom <= body_bottom - radius;
            let core_x = match self.shape {
                ShapeMode::Notch if fully_inside_y => Some((body_left, body_right)),
                ShapeMode::Notch if pixel_bottom <= body_bottom => {
                    Some((body_left + radius, body_right - radius))
                }
                ShapeMode::Capsule if pixel_bottom <= body_bottom => {
                    if pixel_top >= radius && fully_inside_y {
                        Some((body_left, body_right))
                    } else {
                        Some((body_left + radius, body_right - radius))
                    }
                }
                _ => None,
            };

            let (core_start, core_end) = core_x.map_or((0, 0), |(left, right)| {
                (
                    (left * self.scale).ceil().clamp(0.0, f64::from(width)) as usize,
                    (right * self.scale).floor().clamp(0.0, f64::from(width)) as usize,
                )
            });
            let row_start = y as usize * width as usize;
            let row = &mut alpha[row_start..row_start + width as usize];
            if core_start < core_end {
                row[core_start..core_end].fill(u8::MAX);
            }
            for x in (0..core_start).chain(core_end..width as usize) {
                row[x] = self.pixel_alpha(x as u32, y, samples_per_axis);
            }
        }
        true
    }
}

fn round_rect_signed_distance(
    point: (f64, f64),
    bounds: (f64, f64, f64, f64),
    radius: f64,
    round_top: bool,
) -> f64 {
    let (x, y) = point;
    let (left, top, width, height) = bounds;
    let right = left + width;
    let bottom = top + height;

    if round_top && x < left + radius && y < top + radius {
        return (x - (left + radius)).hypot(y - (top + radius)) - radius;
    }
    if round_top && x > right - radius && y < top + radius {
        return (x - (right - radius)).hypot(y - (top + radius)) - radius;
    }
    if x < left + radius && y > bottom - radius {
        return (x - (left + radius)).hypot(y - (bottom - radius)) - radius;
    }
    if x > right - radius && y > bottom - radius {
        return (x - (right - radius)).hypot(y - (bottom - radius)) - radius;
    }

    signed_distance_rect(point, bounds)
}

fn signed_distance_rect(point: (f64, f64), bounds: (f64, f64, f64, f64)) -> f64 {
    let (x, y) = point;
    let (left, top, width, height) = bounds;
    let right = left + width;
    let bottom = top + height;
    let dx = (left - x).max(0.0).max(x - right);
    let dy = (top - y).max(0.0).max(y - bottom);
    if dx > 0.0 || dy > 0.0 {
        dx.hypot(dy)
    } else {
        -((x - left).min(right - x).min(y - top).min(bottom - y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_pixel_alpha(layout: ShellLayout, x: u32, y: u32, samples: usize) -> u8 {
        let mut coverage = 0.0;
        for sample_y in 0..samples {
            for sample_x in 0..samples {
                let logical_x =
                    (f64::from(x) + (sample_x as f64 + 0.5) / samples as f64) / layout.scale;
                let logical_y =
                    (f64::from(y) + (sample_y as f64 + 0.5) / samples as f64) / layout.scale;
                coverage += layout.coverage(logical_x, logical_y);
            }
        }
        (coverage / (samples * samples) as f64 * 255.0).round() as u8
    }

    #[test]
    fn source_layout_sizes_cover_both_shapes() {
        assert_eq!(IDLE_MIN_WIDTH, 150.0);
        assert_eq!(ShapeMode::Notch.surface_width(FOLDED_STATUS_WIDTH), 202.0);
        assert_eq!(ShapeMode::Capsule.surface_width(FOLDED_STATUS_WIDTH), 202.0);
        assert_eq!(
            ShapeMode::Notch.folded_body_width(),
            CALIBRATED_STATUS_WIDTH
        );
        assert_eq!(ShapeMode::Capsule.folded_body_width(), 202.0);
        assert_eq!(ShapeMode::Notch.expanded_body_width(), 680.0);
        assert_eq!(ShapeMode::Capsule.expanded_body_width(), 680.0);
        assert_eq!(ShapeMode::Notch.top_gap(), 0.0);
        assert_eq!(ShapeMode::Capsule.top_gap(), CAPSULE_GAP);
        assert_eq!((EXPANDED_WIDTH, EXPANDED_HEIGHT), (680.0, 336.0));
        assert_eq!((CHAT_WIDTH, CHAT_HEIGHT), (680.0, 420.0));
    }

    #[test]
    fn centered_shell_and_child_keep_the_exact_center_across_dpi_scales() {
        for screen_width in [1365, 1366, 1920, 1921, 2560] {
            for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
                let logical_widths = std::iter::once(193.6)
                    .chain(std::iter::once(202.0))
                    .chain((210..=680).step_by(7).map(f64::from));
                for logical_width in logical_widths {
                    let shell_width = centered_pixel_width(
                        logical_width * scale,
                        screen_width,
                    ) as i32;
                    let shell_x = centered_pixel_origin(screen_width, shell_width);
                    assert_eq!(2 * shell_x + shell_width, screen_width);

                    let compact_width = centered_pixel_width(
                        (logical_width - 8.0) * scale,
                        screen_width,
                    ) as i32;
                    let compact_x = centered_pixel_origin(shell_width, compact_width);
                    assert_eq!(2 * compact_x + compact_width, shell_width);
                    assert!(compact_width < shell_width);

                    for shape in [ShapeMode::Notch, ShapeMode::Capsule] {
                        let layout = ShellLayout::with_body_height(
                            shape,
                            f64::from(shell_width) / scale,
                            f64::from(FOLDED_HEIGHT),
                            scale,
                        )
                        .expect("quantized width remains a valid shell layout");
                        assert_eq!(layout.physical_size().0 as i32, shell_width);
                    }
                }
            }
        }
    }

    #[test]
    fn lower_radius_tracks_reference_caps() {
        assert_eq!(ShapeMode::Notch.lower_radius(FOLDED_HEIGHT, false), 15.0);
        assert_eq!(ShapeMode::Notch.lower_radius(100.0, false), 26.0);
        assert_eq!(ShapeMode::Capsule.lower_radius(100.0, true), 32.0);
    }

    #[test]
    fn shell_layout_keeps_screenshot_and_source_width_measurements_separate() {
        let notch = ShellLayout::new(ShapeMode::Notch, CALIBRATED_STATUS_WIDTH, 1.25)
            .expect("valid calibrated layout");
        assert_eq!(notch.physical_size(), (242, 42));
        let capsule =
            ShellLayout::new(ShapeMode::Capsule, 202.0, 1.25).expect("valid Capsule layout");
        assert_eq!(capsule.physical_size(), (253, 52));
        assert_eq!(FOLDED_STATUS_WIDTH, 202.0);
        assert_ne!(
            notch.logical_width(),
            f64::from(ShapeMode::Notch.surface_width(FOLDED_STATUS_WIDTH))
        );
    }

    #[test]
    fn expanded_layout_uses_the_shared_surface_width_and_smooth_lower_radius() {
        let expanded_body_width = f64::from(EXPANDED_WIDTH);
        let expanded = ShellLayout::with_body_height(
            ShapeMode::Notch,
            expanded_body_width,
            f64::from(EXPANDED_HEIGHT),
            1.0,
        )
        .expect("valid expanded layout");
        assert_eq!(expanded.logical_width(), f64::from(EXPANDED_WIDTH));
        assert_eq!(expanded.physical_size(), (680, 340));
        assert_eq!(expanded.coverage(0.0, 335.0), 0.0);
        assert_eq!(expanded.coverage(340.0, 335.0), 1.0);

        let capsule = ShellLayout::with_body_height(
            ShapeMode::Capsule,
            ShapeMode::Capsule.expanded_body_width(),
            f64::from(EXPANDED_HEIGHT),
            1.0,
        )
        .expect("valid expanded Capsule layout");
        assert_eq!(capsule.logical_width(), f64::from(EXPANDED_WIDTH));
        assert_eq!(capsule.physical_size(), (680, 348));
        assert_eq!(capsule.coverage(340.0, 7.0), 0.0);
        assert_eq!(capsule.coverage(340.0, 8.0), 1.0);
    }

    #[test]
    fn capsule_gap_is_transparent_in_render_and_hit_alpha() {
        let capsule =
            ShellLayout::new(ShapeMode::Capsule, 202.0, 1.0).expect("valid Capsule layout");
        let (width, height) = capsule.physical_size();
        let mut alpha = vec![0; width as usize * height as usize];
        assert!(capsule.fill_alpha_plane(&mut alpha, 4));

        for y in 0..CAPSULE_GAP as u32 {
            for x in 0..width {
                assert_eq!(capsule.pixel_alpha(x, y, 4), 0);
                assert_eq!(alpha[y as usize * width as usize + x as usize], 0);
            }
        }
        assert_eq!(
            capsule.pixel_alpha(width / 2, CAPSULE_GAP as u32, 4),
            u8::MAX
        );
    }

    #[test]
    fn expanded_layout_rejects_height_below_the_folded_body() {
        assert!(ShellLayout::with_body_height(ShapeMode::Notch, 193.6, 29.99, 1.0).is_none());
    }

    #[test]
    fn shell_hit_alpha_is_mirrored_and_transparent_outside_the_shape() {
        for shape in [ShapeMode::Notch, ShapeMode::Capsule] {
            let layout = ShellLayout::new(shape, CALIBRATED_STATUS_WIDTH, 1.25)
                .expect("valid calibrated layout");
            let (width, height) = layout.physical_size();
            for y in 0..height {
                for x in 0..width {
                    assert_eq!(
                        layout.pixel_alpha(x, y, 8),
                        layout.pixel_alpha(width - 1 - x, y, 8),
                        "asymmetric alpha at ({x},{y}) for {shape:?}"
                    );
                }
            }
            assert_eq!(layout.pixel_alpha(0, height - 1, 8), 0);
        }
    }

    #[test]
    fn opaque_pixel_fast_path_matches_supersampled_reference_near_contours() {
        for (shape, height) in [
            (ShapeMode::Notch, f64::from(FOLDED_HEIGHT)),
            (ShapeMode::Notch, f64::from(EXPANDED_HEIGHT)),
            (ShapeMode::Capsule, f64::from(FOLDED_HEIGHT)),
            (ShapeMode::Capsule, f64::from(EXPANDED_HEIGHT)),
        ] {
            let layout = ShellLayout::with_body_height(shape, 193.6, height, 1.25)
                .expect("valid contour test geometry");
            let (width, physical_height) = layout.physical_size();
            let x_step = (width / 41).max(1);
            let y_step = (physical_height / 37).max(1);
            for y in (0..physical_height).step_by(y_step as usize) {
                for x in (0..width).step_by(x_step as usize) {
                    assert_eq!(
                        layout.pixel_alpha(x, y, 4),
                        reference_pixel_alpha(layout, x, y, 4),
                        "fast alpha mismatch at ({x},{y}), {shape:?}, height {height}"
                    );
                }
            }

            let body_edge = (f64::from(shape.body_inset()) * layout.scale).round() as i64;
            let right_edge =
                ((f64::from(shape.body_inset()) + layout.body_width) * layout.scale).round() as i64;
            for y in [0_i64, 1, 12, 16, physical_height as i64 - 2] {
                for x in [
                    body_edge - 2,
                    body_edge - 1,
                    body_edge,
                    body_edge + 1,
                    right_edge - 1,
                    right_edge,
                    right_edge + 1,
                ] {
                    if x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(physical_height) {
                        assert_eq!(
                            layout.pixel_alpha(x as u32, y as u32, 4),
                            reference_pixel_alpha(layout, x as u32, y as u32, 4),
                            "fast alpha mismatch at ({x},{y}), {shape:?}, height {height}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn scanline_alpha_plane_matches_reference_for_both_shell_shapes() {
        for shape in [ShapeMode::Notch, ShapeMode::Capsule] {
            for height in [f64::from(FOLDED_HEIGHT), f64::from(EXPANDED_HEIGHT)] {
                let layout = ShellLayout::with_body_height(shape, 193.6, height, 0.5)
                    .expect("valid full-plane test geometry");
                let (width, physical_height) = layout.physical_size();
                let mut alpha = vec![0; width as usize * physical_height as usize];
                assert!(layout.fill_alpha_plane(&mut alpha, 4));
                for y in 0..physical_height {
                    for x in 0..width {
                        assert_eq!(
                            alpha[y as usize * width as usize + x as usize],
                            reference_pixel_alpha(layout, x, y, 4),
                            "scanline alpha mismatch at ({x},{y}), {shape:?}, height {height}"
                        );
                    }
                }
                let short_len = alpha.len() - 1;
                assert!(!layout.fill_alpha_plane(&mut alpha[..short_len], 4));
                assert!(!layout.fill_alpha_plane(&mut alpha, 0));
            }
        }
    }

    #[test]
    fn invalid_shell_scale_and_width_are_rejected() {
        assert!(ShellLayout::new(ShapeMode::Notch, 149.0, 1.0).is_none());
        assert!(ShellLayout::new(ShapeMode::Notch, 193.6, 0.0).is_none());
        assert!(ShellLayout::new(ShapeMode::Notch, f64::NAN, 1.0).is_none());
    }
}
