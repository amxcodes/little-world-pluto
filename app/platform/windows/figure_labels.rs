use windows_startup::{
    Win32::Graphics::{
        Direct2D::{Common::*, *},
        DirectWrite::*,
        Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGISurface},
    },
    core::{Result, w},
};

pub(super) struct Labels {
    target: ID2D1RenderTarget,
    foreground: ID2D1SolidColorBrush,
    background: ID2D1SolidColorBrush,
    format: IDWriteTextFormat,
}

impl Labels {
    pub(super) fn new(surface: &IDXGISurface) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateDxgiSurfaceRenderTarget(
                surface,
                &D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.,
                    dpiY: 96.,
                    ..Default::default()
                },
            )?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = write.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                13.,
                w!("en-US"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let foreground = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.,
                    g: 1.,
                    b: 1.,
                    a: 1.,
                },
                None,
            )?;
            let background = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.04,
                    g: 0.05,
                    b: 0.07,
                    a: 0.9,
                },
                None,
            )?;
            Ok(Self {
                target,
                foreground,
                background,
                format,
            })
        }
    }

    pub(super) fn draw(
        &self,
        text: &str,
        anchor: [f32; 2],
        size: (u32, u32),
        scale: f32,
        occupied: &mut Vec<[f32; 4]>,
    ) -> Result<()> {
        let width = size.0 as f32 / scale;
        let height = size.1 as f32 / scale;
        if width < 40. || height < 30. {
            return Ok(());
        }
        let text: Vec<u16> = text
            .chars()
            .take(72)
            .collect::<String>()
            .encode_utf16()
            .collect();
        let box_width = (text.len() as f32 * 8. + 18.).min(width - 8.);
        let anchor = [anchor[0] / scale, anchor[1] / scale];
        let [left, top, right, bottom] = layout(anchor, box_width, width, height, occupied);
        let background = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left,
                top,
                right,
                bottom,
            },
            radiusX: 7.,
            radiusY: 7.,
        };
        let rect = D2D_RECT_F {
            left: left + 8.,
            top: top + 3.,
            right: left + box_width - 6.,
            bottom: top + 23.,
        };
        unsafe {
            self.target.SetDpi(96. * scale, 96. * scale);
            self.target.BeginDraw();
            let end = [anchor[0].clamp(left, right), anchor[1].clamp(top, bottom)];
            if (anchor[0] - end[0]).abs() > 0.5 {
                self.target.FillRectangle(
                    &D2D_RECT_F {
                        left: anchor[0].min(end[0]),
                        right: anchor[0].max(end[0]),
                        top: anchor[1] - 0.5,
                        bottom: anchor[1] + 0.5,
                    },
                    &self.foreground,
                );
            }
            if (anchor[1] - end[1]).abs() > 0.5 {
                self.target.FillRectangle(
                    &D2D_RECT_F {
                        left: end[0] - 0.5,
                        right: end[0] + 0.5,
                        top: anchor[1].min(end[1]),
                        bottom: anchor[1].max(end[1]),
                    },
                    &self.foreground,
                );
            }
            self.target
                .FillRoundedRectangle(&background, &self.background);
            self.target.DrawText(
                &text,
                &self.format,
                &rect,
                &self.foreground,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.target.EndDraw(None, None)
        }
    }
}

fn layout(
    anchor: [f32; 2],
    requested_width: f32,
    width: f32,
    height: f32,
    occupied: &mut Vec<[f32; 4]>,
) -> [f32; 4] {
    let box_width = requested_width.min(width - 8.);
    let mut best = [0.; 4];
    let mut best_overlap = f32::INFINITY;
    for side in [1, -1] {
        for row in 0..=24 {
            let shift = if row == 0 {
                0.
            } else {
                ((row + 1) / 2) as f32 * 28. * if row % 2 == 1 { -1. } else { 1. }
            };
            let left = (if side == 1 {
                anchor[0] + 10.
            } else {
                anchor[0] - box_width - 10.
            })
            .clamp(4., width - box_width - 4.);
            let top = (anchor[1] - 32. + shift).clamp(4., height - 28.);
            let candidate = [left, top, left + box_width, top + 24.];
            let overlap: f32 = occupied
                .iter()
                .map(|other| {
                    ((candidate[2] + 2.).min(other[2]) - (candidate[0] - 2.).max(other[0])).max(0.)
                        * ((candidate[3] + 2.).min(other[3]) - (candidate[1] - 2.).max(other[1]))
                            .max(0.)
                })
                .sum();
            if overlap < best_overlap {
                best = candidate;
                best_overlap = overlap;
            }
            if overlap == 0. {
                occupied.push(candidate);
                return candidate;
            }
        }
    }
    occupied.push(best);
    best
}

pub(super) fn project(
    point: [f32; 3],
    pose: [f32; 16],
    view: [f32; 16],
    size: (u32, u32),
) -> Option<[f32; 2]> {
    let transform = |matrix: [f32; 16], point: [f32; 4]| {
        std::array::from_fn::<_, 4, _>(|row| {
            (0..4)
                .map(|column| matrix[column * 4 + row] * point[column])
                .sum::<f32>()
        })
    };
    let clip = transform(view, transform(pose, [point[0], point[1], point[2], 1.]));
    if !clip.iter().all(|value| value.is_finite()) || clip[3] <= 0. {
        return None;
    }
    let [x, y, z] = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
    if [x, y, z].iter().any(|value| !(-1.0..=1.0).contains(value)) {
        return None;
    }
    Some([
        (x + 1.) * 0.5 * size.0 as f32,
        (1. - y) * 0.5 * size.1 as f32,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearby_labels_choose_separate_rectangles_inside_the_viewport() {
        let mut occupied = Vec::new();
        for _ in 0..5 {
            let rect = layout([160., 100.], 100., 320., 220., &mut occupied);
            assert!(rect[0] >= 4. && rect[1] >= 4. && rect[2] <= 316. && rect[3] <= 216.);
        }
        for (index, a) in occupied.iter().enumerate() {
            for b in &occupied[index + 1..] {
                assert!(a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]);
            }
        }
    }
    #[test]
    fn label_anchor_follows_parent_pose_and_uses_viewport_coordinates() {
        let mut identity = [0.; 16];
        for i in [0, 5, 10, 15] {
            identity[i] = 1.;
        }
        assert_eq!(
            project([0.; 3], identity, identity, (200, 100)),
            Some([100., 50.])
        );
        let mut pose = identity;
        pose[12] = 0.5;
        pose[13] = 0.5;
        assert_eq!(
            project([0.; 3], pose, identity, (200, 100)),
            Some([150., 25.])
        );
        assert!(project([2., 0., 0.], identity, identity, (200, 100)).is_none());
    }
}
