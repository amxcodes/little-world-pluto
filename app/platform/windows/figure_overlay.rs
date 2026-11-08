use std::{collections::HashMap, ffi::c_void, slice};

use windows_startup as windows;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE_HARDWARE, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Fxc::D3DCompile,
                ID3DBlob,
            },
            Direct3D11::*,
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
            },
            Dxgi::{Common::*, *},
        },
    },
    core::{Error, Interface, PCSTR, s},
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_TOPMOST, LWA_ALPHA, SW_HIDE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};

const SHADER: &[u8] = br#"
cbuffer Scene : register(b0) {
    column_major float4x4 viewProjection;
    column_major float4x4 pose;
    float4 color;
    float4 finish;
    float4 viewDirection;
};
struct Vertex { float3 position : POSITION; float3 normal : NORMAL; };
struct Pixel { float4 position : SV_POSITION; float3 normal : NORMAL; float3 bindPosition : TEXCOORD0; float3 bindNormal : TEXCOORD1; };
Pixel vs(Vertex vertex) {
    Pixel output;
    output.position = mul(viewProjection, mul(pose, float4(vertex.position, 1)));
    output.position.z = (output.position.z + output.position.w) * 0.5;
    output.bindPosition = vertex.position;
    output.bindNormal = vertex.normal;
    output.normal = mul((float3x3)pose, vertex.normal);
    return output;
}
float finishGrain(Pixel pixel) {
    if (finish.z < 0.5) return 1.0;
    float3 n = abs(pixel.bindNormal);
    float3 p = pixel.bindPosition;
    float2 uv = n.x > n.y && n.x > n.z ? p.zy : n.y > n.z ? p.xz : p.xy;
    float2 grid = uv * 64.0;
    float footprint = max(fwidth(grid.x), fwidth(grid.y));
    float detail = 1.0 - smoothstep(0.35, 1.5, footprint);
    float noise = frac(sin(dot(floor(grid), float2(12.9898, 78.233))) * 43758.5453);
    if (finish.z < 1.5) {
        return 0.93 + detail * (0.028 * sin(grid.y * 1.570796) + 0.018 * (noise - 0.5));
    }
    if (finish.z < 2.5) return 0.93 + detail * 0.12 * (noise - 0.5);
    return 0.96 + detail * 0.04 * (noise - 0.5);
}
float4 ps(Pixel pixel) : SV_TARGET {
    float3 n = normalize(pixel.normal);
    n *= dot(n, viewDirection.xyz) >= 0.0 ? 1.0 : -1.0;
    float3 l = normalize(float3(-0.4, 0.8, 0.6));
    float3 h = normalize(l + viewDirection.xyz + 0.00001);
    float roughness = clamp(finish.x, 0.08, 1.0);
    float metalness = saturate(finish.y);
    float diffuse = 0.38 + 0.55 * saturate(dot(n, l));
    float spec = pow(saturate(dot(n, h)), lerp(96.0, 5.0, roughness))
        * (1.0 - 0.65 * roughness) * saturate(dot(n, l));
    float3 albedo = pow(saturate(color.rgb), 2.2) * finishGrain(pixel);
    float3 reflected = lerp(float3(0.07, 0.07, 0.07), albedo, metalness);
    float3 shaded = albedo * diffuse * (1.0 - 0.35 * metalness) + reflected * spec;
    float3 srgb = pow(saturate(lerp(shaded, albedo, saturate(color.a))), 1.0 / 2.2);
    return float4(srgb * finish.w, finish.w);
}
"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshUpload {
    pub id: u32,
    pub vertices: String,
    pub indices: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FigureFrame {
    pub rect: [f32; 4],
    pub view_projection: [f32; 16],
    pub draws: Vec<FigureDraw>,
    view_direction: Option<[f32; 3]>,
    timeline: Option<FrameTimeline>,
    #[serde(default)]
    labels: Vec<FrameLabel>,
}

#[derive(Deserialize)]
struct FrameLabel {
    text: String,
    point: [f32; 3],
    node: Option<usize>,
}

#[derive(Deserialize)]
pub struct FigureDraw {
    id: u32,
    node: Option<usize>,
    pose: [f32; 16],
    color: [f32; 4],
    finish: Option<[f32; 2]>,
    grain: Option<u8>,
    opacity: Option<f32>,
    center: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct FrameTimeline {
    nodes: Vec<super::figure_pose::PoseNode>,
    stage: f64,
    elapsed: f64,
    target: Option<f64>,
    stage_seconds: Option<f64>,
    loop_playback: Option<crate::agent_presentation::FigurePlayback>,
    loop_last_stage: Option<f64>,
    #[serde(default)]
    playing: bool,
}

impl FrameTimeline {
    fn valid(&self) -> bool {
        let max_stage = (crate::agent_presentation::MAX_FIGURE_STEPS - 1) as f64;
        (0.0..=max_stage).contains(&self.stage)
            && self
                .stage_seconds
                .is_none_or(|seconds| (0.25 / max_stage..=60.0).contains(&seconds))
            && self
                .target
                .is_none_or(|target| (0.0..=max_stage).contains(&target))
            && self.elapsed.is_finite()
            && self.elapsed >= 0.0
            && self.loop_playback.as_ref().is_none_or(|playback| {
                self.loop_last_stage
                    .zip(self.stage_seconds)
                    .is_some_and(|(last, seconds)| {
                        (1.0..=max_stage).contains(&last) && playback.valid(last * seconds)
                    })
            })
            && self.nodes.len() <= 80
            && self
                .nodes
                .iter()
                .enumerate()
                .all(|(index, node)| node.valid(index))
    }
}

fn opacity(draw: &FigureDraw) -> f32 {
    draw.opacity
        .filter(|alpha| (0.1..=1.0).contains(alpha))
        .unwrap_or(1.)
}

struct Mesh {
    vertices: ID3D11Buffer,
    indices: ID3D11Buffer,
    count: u32,
    bytes: usize,
}

struct Targets {
    color: ID3D11RenderTargetView,
    depth: ID3D11DepthStencilView,
    size: (u32, u32),
    labels: super::figure_labels::Labels,
}

struct OverlayWindow(HWND);
unsafe extern "system" fn overlay_hit_test(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _: usize,
    _: usize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{HTTRANSPARENT, WM_NCHITTEST};
    if message == WM_NCHITTEST {
        return HTTRANSPARENT as isize;
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
impl Drop for OverlayWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0.0) };
    }
}

pub struct FigureOverlay {
    window: OverlayWindow,
    _composition: IDCompositionDevice,
    _target: IDCompositionTarget,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap: IDXGISwapChain1,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    raster: ID3D11RasterizerState,
    glass_blend: ID3D11BlendState,
    glass_depth: ID3D11DepthStencilState,
    constants: ID3D11Buffer,
    targets: Option<Targets>,
    meshes: HashMap<u32, Mesh>,
    bounds: Option<(i32, i32, u32, u32)>,
    offset: (i32, i32),
    playback: Option<(FigureFrame, std::time::Instant)>,
    #[cfg(test)]
    label_probe: Option<u8>,
}

fn missing() -> Error {
    Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))
}

unsafe fn shader(entry: PCSTR, target: PCSTR) -> windows::core::Result<ID3DBlob> {
    let mut code = None;
    unsafe {
        D3DCompile(
            SHADER.as_ptr().cast(),
            SHADER.len(),
            PCSTR::null(),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            None,
        )?;
    }
    code.ok_or_else(missing)
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    unsafe { slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize()) }
}

fn buffer(
    device: &ID3D11Device,
    flag: D3D11_BIND_FLAG,
    size: u32,
    data: Option<&[u8]>,
) -> windows::core::Result<ID3D11Buffer> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: size,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: flag.0 as u32,
        ..Default::default()
    };
    let initial = data.map(|bytes| D3D11_SUBRESOURCE_DATA {
        pSysMem: bytes.as_ptr().cast(),
        ..Default::default()
    });
    let mut output = None;
    unsafe {
        device.CreateBuffer(
            &desc,
            initial.as_ref().map(std::ptr::from_ref),
            Some(&mut output),
        )?
    };
    output.ok_or_else(missing)
}

impl FigureOverlay {
    pub fn new(owner: HWND) -> windows::core::Result<Self> {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.ok_or_else(missing)?;
            let context = context.ok_or_else(missing)?;
            let dxgi: IDXGIDevice = device.cast()?;
            let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
            let swap = factory.CreateSwapChainForComposition(
                &device,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: 1,
                    Height: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 2,
                    Scaling: DXGI_SCALING_STRETCH,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                    ..Default::default()
                },
                None::<&IDXGIOutput>,
            )?;
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TRANSPARENT
                    | WS_EX_LAYERED
                    | WS_EX_NOREDIRECTIONBITMAP,
                windows::core::w!("STATIC").as_ptr(),
                windows::core::w!("Pluto figure").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                owner.0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return Err(Error::from_thread());
            }
            let window = OverlayWindow(HWND(hwnd));
            if SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) == 0 {
                return Err(Error::from_thread());
            }
            if SetWindowSubclass(hwnd, Some(overlay_hit_test), 1, 0) == 0 {
                return Err(Error::from_thread());
            }
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
            let target = composition.CreateTargetForHwnd(window.0, true)?;
            let visual = composition.CreateVisual()?;
            visual.SetContent(&swap)?;
            target.SetRoot(&visual)?;
            composition.Commit()?;

            let vertex = shader(s!("vs"), s!("vs_4_0"))?;
            let pixel = shader(s!("ps"), s!("ps_4_0"))?;
            let mut vertex_shader = None;
            let mut pixel_shader = None;
            let mut layout = None;
            let mut raster = None;
            device.CreateVertexShader(
                blob_bytes(&vertex),
                None::<&ID3D11ClassLinkage>,
                Some(&mut vertex_shader),
            )?;
            device.CreatePixelShader(
                blob_bytes(&pixel),
                None::<&ID3D11ClassLinkage>,
                Some(&mut pixel_shader),
            )?;
            device.CreateInputLayout(
                &[
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("POSITION"),
                        Format: DXGI_FORMAT_R32G32B32_FLOAT,
                        ..Default::default()
                    },
                    D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: s!("NORMAL"),
                        Format: DXGI_FORMAT_R32G32B32_FLOAT,
                        AlignedByteOffset: 12,
                        ..Default::default()
                    },
                ],
                blob_bytes(&vertex),
                Some(&mut layout),
            )?;
            device.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut raster),
            )?;
            let mut blend = D3D11_BLEND_DESC::default();
            blend.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut glass_blend = None;
            device.CreateBlendState(&blend, Some(&mut glass_blend))?;
            let mut glass_depth = None;
            device.CreateDepthStencilState(
                &D3D11_DEPTH_STENCIL_DESC {
                    DepthEnable: true.into(),
                    DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                    DepthFunc: D3D11_COMPARISON_LESS_EQUAL,
                    ..Default::default()
                },
                Some(&mut glass_depth),
            )?;
            let constants = buffer(&device, D3D11_BIND_CONSTANT_BUFFER, 176, None)?;
            Ok(Self {
                window,
                _composition: composition,
                _target: target,
                device,
                context,
                swap,
                vertex_shader: vertex_shader.ok_or_else(missing)?,
                pixel_shader: pixel_shader.ok_or_else(missing)?,
                layout: layout.ok_or_else(missing)?,
                raster: raster.ok_or_else(missing)?,
                glass_blend: glass_blend.ok_or_else(missing)?,
                glass_depth: glass_depth.ok_or_else(missing)?,
                constants,
                targets: None,
                meshes: HashMap::new(),
                bounds: None,
                offset: (0, 0),
                playback: None,
                #[cfg(test)]
                label_probe: None,
            })
        }
    }

    pub fn hide(&mut self) {
        self.playback = None;
        unsafe { ShowWindow(self.window.0.0, SW_HIDE) };
        self.bounds = None;
    }

    pub fn is_presenting(&self) -> bool {
        self.bounds.is_some()
    }

    pub fn follow_owner(&mut self, origin: (i32, i32)) -> windows::core::Result<()> {
        if let Some((_, _, width, height)) = self.bounds {
            self.place(origin, width, height)?;
        }
        Ok(())
    }

    fn place(&mut self, origin: (i32, i32), width: u32, height: u32) -> windows::core::Result<()> {
        let bounds = (
            origin.0 + self.offset.0,
            origin.1 + self.offset.1,
            width,
            height,
        );
        if self.bounds != Some(bounds) {
            if unsafe {
                SetWindowPos(
                    self.window.0.0,
                    HWND_TOPMOST,
                    bounds.0,
                    bounds.1,
                    width as i32,
                    height as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            } == 0
            {
                return Err(Error::from_thread());
            }
            self.bounds = Some(bounds);
        }
        Ok(())
    }

    pub fn upload(&mut self, upload: MeshUpload) -> Option<()> {
        if self.meshes.len() >= 160
            || upload.vertices.len() > 2_800_000
            || upload.indices.len() > 2_400_000
        {
            return None;
        }
        let vertices = STANDARD.decode(upload.vertices).ok()?;
        let indices = STANDARD.decode(upload.indices).ok()?;
        let bytes = vertices.len() + indices.len();
        let existing = self.meshes.values().map(|mesh| mesh.bytes).sum::<usize>();
        if vertices.is_empty()
            || indices.is_empty()
            || existing + bytes > 24 * 1024 * 1024
            || vertices.len() % 24 != 0
            || indices.len() % 12 != 0
            || vertices
                .chunks_exact(4)
                .any(|n| !f32::from_le_bytes(n.try_into().unwrap()).is_finite())
            || indices
                .chunks_exact(4)
                .any(|n| u32::from_le_bytes(n.try_into().unwrap()) as usize >= vertices.len() / 24)
        {
            return None;
        }
        let mesh = Mesh {
            vertices: buffer(
                &self.device,
                D3D11_BIND_VERTEX_BUFFER,
                vertices.len() as u32,
                Some(&vertices),
            )
            .ok()?,
            indices: buffer(
                &self.device,
                D3D11_BIND_INDEX_BUFFER,
                indices.len() as u32,
                Some(&indices),
            )
            .ok()?,
            count: (indices.len() / 4) as u32,
            bytes,
        };
        self.meshes.insert(upload.id, mesh);
        Some(())
    }

    fn resize(&mut self, width: u32, height: u32) -> windows::core::Result<()> {
        if self
            .targets
            .as_ref()
            .is_some_and(|target| target.size == (width, height))
        {
            return Ok(());
        }
        unsafe {
            self.context
                .OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            self.targets = None;
            self.swap.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
            let back: ID3D11Texture2D = self.swap.GetBuffer(0)?;
            let surface: IDXGISurface = back.cast()?;
            let labels = super::figure_labels::Labels::new(&surface)?;
            let mut color = None;
            self.device
                .CreateRenderTargetView(&back, None, Some(&mut color))?;
            let mut depth_texture = None;
            self.device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_D32_FLOAT,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut depth_texture),
            )?;
            let mut depth = None;
            self.device.CreateDepthStencilView(
                &depth_texture.ok_or_else(missing)?,
                None,
                Some(&mut depth),
            )?;
            self.targets = Some(Targets {
                color: color.ok_or_else(missing)?,
                depth: depth.ok_or_else(missing)?,
                size: (width, height),
                labels,
            });
        }
        Ok(())
    }

    pub fn retain_playback(&mut self, frame: FigureFrame) -> bool {
        let active = self.is_presenting()
            && frame.timeline.as_ref().is_some_and(|timeline| {
                timeline.playing
                    && timeline.valid()
                    && (timeline
                        .target
                        .is_some_and(|target| target != timeline.stage)
                        || timeline.loop_playback.is_some()
                        || timeline
                            .nodes
                            .iter()
                            .any(|node| node.animates_at(timeline.elapsed)))
            });
        self.playback = active.then(|| (frame, std::time::Instant::now()));
        self.playback.is_some()
    }

    pub fn advance_playback(
        &mut self,
        origin: (i32, i32),
        scale: f32,
        viewport: (u32, u32),
    ) -> windows::core::Result<bool> {
        let Some((mut frame, previous)) = self.playback.take() else {
            return Ok(false);
        };
        let now = std::time::Instant::now();
        let delta = now.duration_since(previous).as_secs_f64().min(0.04);
        let timeline = frame
            .timeline
            .as_mut()
            .expect("retained playback has a timeline");
        let target = timeline.target.unwrap_or(timeline.stage);
        let advance = delta / timeline.stage_seconds.unwrap_or(0.8);
        timeline.elapsed += delta;
        if let (Some(playback), Some(last), Some(seconds)) = (
            &timeline.loop_playback,
            timeline.loop_last_stage,
            timeline.stage_seconds,
        ) {
            timeline.stage = last * playback.progress(timeline.elapsed, last * seconds);
        } else {
            timeline.stage += (target - timeline.stage).clamp(-advance, advance);
        }
        let still_moving = timeline.loop_playback.is_some()
            || timeline.stage != target
            || timeline
                .nodes
                .iter()
                .any(|node| node.animates_at(timeline.elapsed));
        let busy = self.draw(&frame, origin, scale, viewport)?;
        let keep = busy || self.is_presenting() && still_moving;
        if keep {
            self.playback = Some((frame, now));
        }
        Ok(keep)
    }

    pub fn draw(
        &mut self,
        frame: &FigureFrame,
        origin: (i32, i32),
        scale: f32,
        viewport: (u32, u32),
    ) -> windows::core::Result<bool> {
        let [left, top, width, height] = frame.rect.map(|value| value * scale);
        if ![left, top, width, height].into_iter().all(f32::is_finite)
            || left < 0.0
            || top < 0.0
            || width < 2.0
            || height < 2.0
            || width > 2048.0
            || height > 2048.0
            || left + width > viewport.0 as f32 + 1.0
            || top + height > viewport.1 as f32 + 1.0
            || frame.draws.is_empty()
            || frame.draws.len() > 80
            || !frame.view_projection.iter().all(|value| value.is_finite())
            || frame.draws.iter().any(|draw| {
                !self.meshes.contains_key(&draw.id)
                    || draw
                        .pose
                        .iter()
                        .chain(&draw.color)
                        .any(|value| !value.is_finite())
            })
        {
            self.hide();
            return Ok(false);
        }
        let timeline = frame.timeline.as_ref().filter(|timeline| timeline.valid());
        let poses = timeline.map(|timeline| {
            super::figure_pose::sample(&timeline.nodes, timeline.stage, timeline.elapsed)
        });
        let visibility = timeline.map(|timeline| {
            super::figure_pose::visibility(&timeline.nodes, timeline.stage, timeline.elapsed)
        });
        let (w, h) = (width.round() as u32, height.round() as u32);
        self.resize(w, h)?;
        let target = self.targets.as_ref().ok_or_else(missing)?;
        unsafe {
            self.context
                .OMSetRenderTargets(Some(&[Some(target.color.clone())]), &target.depth);
            self.context.ClearRenderTargetView(&target.color, &[0.0; 4]);
            self.context
                .ClearDepthStencilView(&target.depth, D3D11_CLEAR_DEPTH.0, 1.0, 0);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: w as f32,
                Height: h as f32,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            self.context.RSSetState(&self.raster);
            self.context.IASetInputLayout(&self.layout);
            self.context
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex_shader, None);
            self.context.PSSetShader(&self.pixel_shader, None);
            self.context
                .VSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            self.context
                .PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            let mut constants = [0.0_f32; 44];
            let view = frame
                .view_direction
                .filter(|v| v.iter().all(|value| value.is_finite()))
                .unwrap_or([0., 0., 1.]);
            constants[40..43].copy_from_slice(&view);
            constants[..16].copy_from_slice(&frame.view_projection);
            let pose_for = |draw: &FigureDraw| {
                poses
                    .as_ref()
                    .and_then(|poses| draw.node.and_then(|node| poses.get(node)))
                    .copied()
                    .unwrap_or(draw.pose)
            };
            let mut ordered: Vec<_> = frame
                .draws
                .iter()
                .map(|draw| {
                    let pose = pose_for(draw);
                    let center = draw
                        .center
                        .filter(|center| center.iter().all(|v| v.is_finite()))
                        .unwrap_or([0.; 3]);
                    let depth: f32 = (0..3)
                        .map(|row| {
                            view[row]
                                * (pose[12 + row]
                                    + (0..3)
                                        .map(|col| pose[col * 4 + row] * center[col])
                                        .sum::<f32>())
                        })
                        .sum();
                    (draw, pose, depth)
                })
                .collect();
            ordered.sort_by(|(a, _, da), (b, _, db)| {
                let translucent = |draw: &FigureDraw| opacity(draw) < 1.;
                translucent(a)
                    .cmp(&translucent(b))
                    .then_with(|| da.total_cmp(db))
            });
            self.context
                .OMSetBlendState(None::<&ID3D11BlendState>, None, u32::MAX);
            self.context
                .OMSetDepthStencilState(None::<&ID3D11DepthStencilState>, 0);
            let mut glass_pass = false;
            for (draw, pose, _) in ordered {
                if !glass_pass && opacity(draw) < 1. {
                    self.context
                        .OMSetBlendState(&self.glass_blend, None, u32::MAX);
                    self.context.OMSetDepthStencilState(&self.glass_depth, 0);
                    glass_pass = true;
                }
                if draw
                    .node
                    .and_then(|node| visibility.as_ref()?.get(node))
                    .copied()
                    == Some(false)
                {
                    continue;
                }
                let mesh = &self.meshes[&draw.id];
                constants[16..32].copy_from_slice(&pose);
                let color = timeline
                    .and_then(|timeline| {
                        let node = timeline.nodes.get(draw.node?)?;
                        Some(super::figure_pose::appearance(
                            node,
                            timeline.stage,
                            timeline.elapsed,
                            draw.color,
                        ))
                    })
                    .unwrap_or(draw.color);
                constants[32..36].copy_from_slice(&color);
                let finish = draw
                    .finish
                    .filter(|finish| finish.iter().all(|v| (0.0..=1.0).contains(v)))
                    .unwrap_or([0.67, 0.16]);
                constants[36..38].copy_from_slice(&finish);
                constants[38] = f32::from(draw.grain.filter(|grain| *grain <= 3).unwrap_or(0));
                constants[39] = opacity(draw);
                self.context.UpdateSubresource(
                    &self.constants,
                    0,
                    None,
                    constants.as_ptr().cast::<c_void>(),
                    0,
                    0,
                );
                self.context.IASetVertexBuffers(
                    0,
                    1,
                    Some(&Some(mesh.vertices.clone())),
                    Some(&24),
                    Some(&0),
                );
                self.context
                    .IASetIndexBuffer(&mesh.indices, DXGI_FORMAT_R32_UINT, 0);
                self.context.DrawIndexed(mesh.count, 0, 0);
            }
            let mut occupied = Vec::new();
            for label in frame.labels.iter().take(12) {
                if label.text.is_empty()
                    || label.text.chars().count() > 72
                    || !label
                        .point
                        .iter()
                        .all(|value| value.is_finite() && value.abs() <= 4096.)
                    || label.node.is_some_and(|node| {
                        visibility.as_ref().and_then(|items| items.get(node)) != Some(&true)
                    })
                {
                    continue;
                }
                let pose = label
                    .node
                    .and_then(|node| poses.as_ref()?.get(node))
                    .copied()
                    .unwrap_or([
                        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
                    ]);
                if let Some(anchor) =
                    super::figure_labels::project(label.point, pose, frame.view_projection, (w, h))
                {
                    self.context
                        .OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
                    target
                        .labels
                        .draw(&label.text, anchor, (w, h), scale, &mut occupied)?;
                }
            }
            #[cfg(test)]
            if !frame.labels.is_empty() && w >= 16 && h >= 48 {
                self.label_probe = Some(read_alpha(
                    &self.device,
                    &self.context,
                    &target.color,
                    8,
                    40,
                )?);
            }
            let present = self.swap.Present(0, DXGI_PRESENT_DO_NOT_WAIT);
            if present == DXGI_ERROR_WAS_STILL_DRAWING {
                return Ok(true);
            }
            present.ok()?;
        }
        self.offset = (left.round() as i32, top.round() as i32);
        self.place(origin, w, h)?;
        Ok(false)
    }
}

#[cfg(test)]
fn read_alpha(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    target: &ID3D11RenderTargetView,
    x: usize,
    y: usize,
) -> windows::core::Result<u8> {
    unsafe {
        let texture: ID3D11Texture2D = target.GetResource()?.cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.ok_or_else(missing)?;
        context.CopyResource(&staging, &texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let alpha = *mapped
            .pData
            .cast::<u8>()
            .add(y * mapped.RowPitch as usize + x * 4 + 3);
        context.Unmap(&staging, 0);
        Ok(alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a Windows desktop session and a Direct3D 11 device"]
    fn presents_native_figure_frame() {
        let mut overlay = FigureOverlay::new(HWND::default()).expect("native overlay");
        let vertices: [f32; 18] = [
            -0.5, -0.5, 0.0, 0.0, 0.0, 1.0, 0.5, -0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 0.0,
            1.0,
        ];
        overlay
            .upload(MeshUpload {
                id: 1,
                vertices: STANDARD.encode(
                    vertices
                        .into_iter()
                        .flat_map(f32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
                indices: STANDARD.encode(
                    [0_u32, 1, 2]
                        .into_iter()
                        .flat_map(u32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
            })
            .expect("triangle mesh");
        let mut identity = [0.0; 16];
        for i in [0, 5, 10, 15] {
            identity[i] = 1.0;
        }
        overlay
            .draw(
                &FigureFrame {
                    rect: [0.0, 0.0, 128.0, 128.0],
                    view_projection: identity,
                    view_direction: None,
                    timeline: None,
                    labels: vec![FrameLabel {
                        text: "Native label".into(),
                        point: [0.; 3],
                        node: None,
                    }],
                    draws: vec![FigureDraw {
                        id: 1,
                        node: None,
                        pose: identity,
                        color: [0.8, 0.4, 0.2, 1.0],
                        finish: None,
                        grain: None,
                        opacity: None,
                        center: None,
                    }],
                },
                (-4096, -4096),
                1.0,
                (128, 128),
            )
            .expect("present frame");
        assert!(overlay.bounds.is_some(), "the frame reached the compositor");
        let frame = FigureFrame {
            rect: [0.0, 0.0, 128.0, 128.0],
            view_projection: identity,
            view_direction: None,
            draws: vec![FigureDraw {
                id: 1,
                node: Some(0),
                pose: identity,
                color: [1.; 4],
                finish: Some([0.3, 0.72]),
                grain: Some(1),
                opacity: Some(0.38),
                center: Some([0.; 3]),
            }],
            timeline: Some(FrameTimeline {
                nodes: serde_json::from_value(serde_json::json!([
                    {"step":0,"motion":{"pivot":[0,0,0],"keys":[{"step":1,"rotate":[0,0,90]}]}}
                ]))
                .unwrap(),
                stage: 0.,
                target: Some(1.),
                stage_seconds: None,
                loop_playback: None,
                loop_last_stage: None,
                elapsed: 0.,
                playing: true,
            }),
            labels: vec![
                FrameLabel {
                    text: "Animated part".into(),
                    point: [0.; 3],
                    node: Some(0),
                },
                FrameLabel {
                    text: "Fixed point".into(),
                    point: [0.5, 0.5, 0.],
                    node: None,
                },
            ],
        };
        assert!(overlay.retain_playback(frame));
        use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_APP};
        let message = WM_APP + 1;
        let window = overlay.window.0.0;
        let clock = super::super::figure_clock::start(window, message).unwrap();
        clock.send(std::time::Duration::from_millis(16)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut received = MSG::default();
        while unsafe { PeekMessageW(&mut received, window, message, message, PM_REMOVE) } == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "clock did not post a tick"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        drop(clock);
        overlay.playback.as_mut().unwrap().1 -= std::time::Duration::from_millis(16);
        assert!(
            overlay
                .advance_playback((-4096, -4096), 1., (128, 128))
                .unwrap()
        );
        let (frame, _) = overlay.playback.as_ref().unwrap();
        assert!(frame.timeline.as_ref().unwrap().stage > 0.);
        // This pixel is outside the triangle but inside the label background.
        // Read back the composed surface to prove that Direct2D painted it.
        assert!(
            overlay.label_probe.is_some_and(|alpha| alpha > 200),
            "native label background was absent: alpha {:?}",
            overlay.label_probe
        );
        overlay.hide();
        assert!(overlay.bounds.is_none());
        assert!(
            !overlay
                .advance_playback((-4096, -4096), 1., (128, 128))
                .unwrap()
        );
    }
}
