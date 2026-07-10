//! Manual visual gate for imgui-painter phase 1 (design doc §12 step 1):
//! renders three hand-built "looks" — a macOS-style panel, a Fluent-style
//! button, a GitHub-style button — via imgui-painter, each next to a
//! plain-`ImDrawList` attempt at the same look, so a human can judge
//! whether Painter alone (gradients/shadows/borders on `rounded_rect`,
//! nothing above it) renders convincingly. That judgment is the pass/fail
//! gate for everything the design doc builds on top of Painter.
//!
//! A standalone winit/wgpu/imgui window, deliberately **not** wired into
//! punks-standalone's product code (see the plan's Non-Goals) — this is
//! scaffolding for imgui-painter's own incubation inside punks2, not a
//! punks feature. Run with:
//!   cargo run -p punks-standalone --example painter_demo

use std::sync::Arc;
use std::time::Instant;

use imgui::FontSource;
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::WinitPlatform;
use pollster::block_on;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::Window,
};

use imgui_painter::{
    adapter, rgba, Border, ColorStop, Gradient, GradientMode, Rect as PainterRect, Session, Shadow,
    Vec2 as PainterVec2,
};

fn pv2(x: f32, y: f32) -> PainterVec2 {
    PainterVec2 { x, y }
}

struct GpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
}

struct ImguiState {
    context: imgui::Context,
    platform: WinitPlatform,
    renderer: Renderer,
    last_frame: Instant,
    last_cursor: Option<imgui::MouseCursor>,
}

struct AppWindow {
    gpu: GpuState,
    imgui: ImguiState,
    painter: Session,
}

impl AppWindow {
    fn new(event_loop: &ActiveEventLoop) -> Self {
        let gpu = Self::init_gpu(event_loop);
        let imgui = Self::init_imgui(&gpu);
        AppWindow {
            gpu,
            imgui,
            painter: Session::new(),
        }
    }

    fn init_gpu(event_loop: &ActiveEventLoop) -> GpuState {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let size = LogicalSize::new(1000.0, 700.0);
        let attributes = Window::default_attributes()
            .with_inner_size(size)
            .with_title("imgui-painter phase 1 demo");
        let window = Arc::new(event_loop.create_window(attributes).unwrap());

        let phys_size = window.inner_size();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter found");

        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("failed to create GPU device");

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: phys_size.width.max(1),
            height: phys_size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        };
        surface.configure(&device, &surface_config);

        GpuState {
            device,
            queue,
            window,
            surface,
            surface_config,
        }
    }

    fn init_imgui(gpu: &GpuState) -> ImguiState {
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);

        let mut platform = WinitPlatform::new(&mut context);
        platform.attach_window(
            context.io_mut(),
            &gpu.window,
            imgui_winit_support::HiDpiMode::Default,
        );

        let hidpi = gpu.window.scale_factor();
        let font_size = (14.0 * hidpi) as f32;
        context.io_mut().font_global_scale = (1.0 / hidpi) as f32;

        context.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

        let renderer_config = RendererConfig {
            texture_format: gpu.surface_config.format,
            ..Default::default()
        };
        let renderer = Renderer::new(&mut context, &gpu.device, &gpu.queue, renderer_config);

        ImguiState {
            context,
            platform,
            renderer,
            last_frame: Instant::now(),
            last_cursor: None,
        }
    }
}

// --- The three looks: `_plain` draws with vanilla imgui-rs ImDrawList
// calls, `_painted` draws the same intent through imgui-painter. Colors
// deliberately match between the pair so the only variable a viewer judges
// is the rendering technique, not a color choice. ---

fn draw_macos_panel_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(240, 240, 242, 255))
        .filled(true)
        .rounding(12.0)
        .build();
    draw_list
        .add_rect(pos, max, rgba(210, 210, 214, 255))
        .rounding(12.0)
        .thickness(1.0)
        .build();
}

fn draw_macos_panel_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 12.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 6.0),
        blur: 24.0,
        spread: 2.0,
        color: rgba(0, 0, 0, 60),
        inset: false,
    });
    painter.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: pv2(pos[0], pos[1]),
        to: pv2(pos[0], pos[1] + size[1]),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: rgba(248, 248, 250, 255),
            },
            ColorStop {
                t: 1.0,
                color: rgba(228, 228, 232, 255),
            },
        ],
    });
    painter.add_border(&Border {
        thickness: 1.0,
        color: rgba(210, 210, 214, 255),
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

fn draw_fluent_button_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(0, 103, 192, 255))
        .filled(true)
        .rounding(4.0)
        .build();
}

fn draw_fluent_button_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 4.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 2.0),
        blur: 6.0,
        spread: 0.0,
        color: rgba(0, 0, 0, 70),
        inset: false,
    });
    painter.fill_gradient(&Gradient {
        mode: GradientMode::Linear,
        from: pv2(pos[0], pos[1]),
        to: pv2(pos[0], pos[1] + size[1]),
        stops: vec![
            ColorStop {
                t: 0.0,
                color: rgba(0, 120, 215, 255),
            },
            ColorStop {
                t: 1.0,
                color: rgba(0, 90, 180, 255),
            },
        ],
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

fn draw_github_button_plain(ui: &imgui::Ui, pos: [f32; 2], size: [f32; 2]) {
    let max = [pos[0] + size[0], pos[1] + size[1]];
    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_rect(pos, max, rgba(246, 248, 250, 255))
        .filled(true)
        .rounding(6.0)
        .build();
    draw_list
        .add_rect(pos, max, rgba(31, 35, 40, 45))
        .rounding(6.0)
        .thickness(1.0)
        .build();
}

fn draw_github_button_painted(
    painter: &mut Session,
    white_uv: PainterVec2,
    dl: *mut imgui::sys::ImDrawList,
    pos: [f32; 2],
    size: [f32; 2],
) {
    let rect = PainterRect {
        min: pv2(pos[0], pos[1]),
        max: pv2(pos[0] + size[0], pos[1] + size[1]),
    };
    painter.begin(white_uv);
    painter.rounded_rect(rect, 6.0);
    painter.add_shadow(&Shadow {
        offset: pv2(0.0, 1.0),
        blur: 2.0,
        spread: 0.0,
        color: rgba(31, 35, 40, 35),
        inset: false,
    });
    painter.fill_color(rgba(246, 248, 250, 255));
    painter.add_border(&Border {
        thickness: 1.0,
        color: rgba(31, 35, 40, 45),
    });
    let mesh = painter.end();
    unsafe { adapter::paint_to_draw_list(dl, &mesh) };
}

const BOX_W: f32 = 220.0;
const BOX_H: f32 = 90.0;
const GAP_X: f32 = 60.0;
const GAP_Y: f32 = 70.0;
const LABEL_H: f32 = 22.0;

fn draw_demo(ui: &imgui::Ui, painter: &mut Session) {
    ui.text("imgui-painter phase 1 \u{2014} three looks gate (design doc \u{a7}12 step 1)");
    ui.text_disabled("Left column: plain ImDrawList.  Right column: imgui-painter.");
    ui.separator();
    ui.spacing();

    // SAFETY: called once per frame while this window's draw list is the
    // active one, matching igGetWindowDrawList's normal per-frame usage.
    let white_uv = unsafe { adapter::white_pixel_uv() };
    let origin = ui.cursor_screen_pos();

    let rows: [(&str, PlainFn, PaintedFn); 3] = [
        (
            "macOS-style panel",
            draw_macos_panel_plain,
            draw_macos_panel_painted,
        ),
        (
            "Fluent-style button",
            draw_fluent_button_plain,
            draw_fluent_button_painted,
        ),
        (
            "GitHub-style button",
            draw_github_button_plain,
            draw_github_button_painted,
        ),
    ];

    for (row, (label, plain_fn, painted_fn)) in rows.into_iter().enumerate() {
        let y = origin[1] + row as f32 * (BOX_H + LABEL_H + GAP_Y);
        ui.set_cursor_screen_pos([origin[0], y]);
        ui.text(label);

        let plain_pos = [origin[0], y + LABEL_H];
        plain_fn(ui, plain_pos, [BOX_W, BOX_H]);

        let painted_pos = [origin[0] + BOX_W + GAP_X, y + LABEL_H];
        // SAFETY: this window's draw list is the currently active one for
        // the duration of this call (same frame, same window scope).
        let dl = unsafe { imgui::sys::igGetWindowDrawList() };
        painted_fn(painter, white_uv, dl, painted_pos, [BOX_W, BOX_H]);
    }

    // The raw draw-list calls above don't advance imgui's own layout
    // cursor; reserve the space explicitly so window sizing/scrolling stays
    // correct.
    let content_bottom = origin[1] + rows.len() as f32 * (BOX_H + LABEL_H + GAP_Y);
    ui.set_cursor_screen_pos([origin[0], content_bottom]);
}

type PlainFn = fn(&imgui::Ui, [f32; 2], [f32; 2]);
type PaintedFn = fn(&mut Session, PainterVec2, *mut imgui::sys::ImDrawList, [f32; 2], [f32; 2]);

#[derive(Default)]
struct App {
    window: Option<AppWindow>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(AppWindow::new(event_loop));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let app = match self.window.as_mut() {
            Some(w) => w,
            None => return,
        };
        let im = &mut app.imgui;

        match &event {
            WindowEvent::Resized(size) => {
                app.gpu.surface_config.width = size.width.max(1);
                app.gpu.surface_config.height = size.height.max(1);
                app.gpu
                    .surface
                    .configure(&app.gpu.device, &app.gpu.surface_config);
            }

            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                im.context.io_mut().update_delta_time(now - im.last_frame);
                im.last_frame = now;

                let frame = match app.gpu.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("dropped frame: {e:?}");
                        return;
                    }
                };

                im.platform
                    .prepare_frame(im.context.io_mut(), &app.gpu.window)
                    .expect("failed to prepare imgui frame");

                let ui = im.context.frame();
                let display_size = ui.io().display_size;
                ui.window("painter_demo")
                    .position([0.0, 0.0], imgui::Condition::Always)
                    .size(display_size, imgui::Condition::Always)
                    .no_decoration()
                    .movable(false)
                    .build(|| {
                        draw_demo(ui, &mut app.painter);
                    });

                if im.last_cursor != ui.mouse_cursor() {
                    im.last_cursor = ui.mouse_cursor();
                    im.platform.prepare_render(ui, &app.gpu.window);
                }

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = app
                    .gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let clear_color = wgpu::Color {
                    r: 0.06,
                    g: 0.06,
                    b: 0.07,
                    a: 1.0,
                };

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    im.renderer
                        .render(
                            im.context.render(),
                            &app.gpu.queue,
                            &app.gpu.device,
                            &mut rpass,
                        )
                        .expect("imgui render failed");
                }

                app.gpu.queue.submit(Some(encoder.finish()));
                frame.present();
            }

            _ => {}
        }

        im.platform.handle_event::<()>(
            im.context.io_mut(),
            &app.gpu.window,
            &Event::WindowEvent { window_id, event },
        );
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = self.window.as_mut() {
            app.gpu.window.request_redraw();
            app.imgui.platform.handle_event::<()>(
                app.imgui.context.io_mut(),
                &app.gpu.window,
                &Event::AboutToWait,
            );
        }
    }
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::default()).unwrap();
}
