// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets {
    pub struct GlyphInfo {
        pub width: usize,
        pub height: usize,
        /// Offset of the mask from the pen position, at `GLYPH_PX`.
        pub left: f32,
        /// Offset of the mask's top from the baseline (negative is up).
        pub top: f32,
        pub advance: f32,
        /// Start of this glyph's mask in the glyph blob.
        pub offset: usize,
    }

    include!(concat!(env!("OUT_DIR"), "/assets.rs"));

    pub const CLOSED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/closed.bin"));
    pub const OPEN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/open.bin"));
    pub const ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon.bin"));
    pub const GLYPH_MASKS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glyphs.bin"));
    pub const SOUND: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sound.bin"));
}

mod audio;
mod render;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Icon, Window, WindowId};

use render::{Image, Layout, Scene};

const WINDOW_SIZE: f64 = 400.0;
const MIN_WINDOW_SIZE: f64 = 200.0;
const FRAME: Duration = Duration::from_millis(16);

fn inflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::inflate::decompress_to_vec(data).expect("embedded asset is valid")
}

/// Linear progress towards 0 or 1 over a fixed duration; eased when drawn.
struct Anim {
    value: f32,
    duration: f32,
}

impl Anim {
    fn new(duration: f32) -> Self {
        Self { value: 0.0, duration }
    }

    fn step(&mut self, on: bool, dt: f32) {
        let delta = dt / self.duration;
        self.value = if on { (self.value + delta).min(1.0) } else { (self.value - delta).max(0.0) };
    }

    fn settled(&self, on: bool) -> bool {
        self.value == if on { 1.0 } else { 0.0 }
    }
}

struct Gfx {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    gfx: Option<Gfx>,
    scene: Scene,
    audio: Option<audio::Player>,
    cursor: Option<PhysicalPosition<f64>>,
    pressed: bool,
    press: Anim,
    active: Anim,
    started: Instant,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            gfx: None,
            scene: Scene::new(
                Image::from_filtered_rgb(&inflate(assets::CLOSED), assets::IMAGE_SIZE),
                Image::from_filtered_rgb(&inflate(assets::OPEN), assets::IMAGE_SIZE),
                inflate(assets::GLYPH_MASKS),
            ),
            audio: None,
            cursor: None,
            pressed: false,
            press: Anim::new(0.08),
            active: Anim::new(0.25),
            started: now,
            last_frame: now,
        }
    }

    fn playing(&self) -> bool {
        self.audio.as_ref().is_some_and(audio::Player::is_playing)
    }

    fn mouth_open(&self) -> bool {
        self.pressed || self.playing()
    }

    fn animating(&self) -> bool {
        let open = self.mouth_open();
        open || !self.press.settled(self.pressed) || !self.active.settled(open)
    }

    fn layout(&self) -> Option<Layout> {
        let size = self.gfx.as_ref()?.window.inner_size();
        Some(Layout::new(size.width as f32, size.height as f32))
    }

    fn over_button(&self) -> bool {
        match (self.cursor, self.layout()) {
            (Some(p), Some(layout)) => layout.button_contains(p.x as f32, p.y as f32),
            _ => false,
        }
    }

    fn on_press(&mut self) {
        self.pressed = true;
        if self.audio.is_none() {
            self.audio = audio::Player::open();
        }
        if let Some(audio) = &self.audio {
            audio.restart();
        }
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let open = self.mouth_open();
        self.press.step(self.pressed, dt);
        self.active.step(open, dt);

        if !self.playing() {
            if let Some(audio) = &self.audio {
                audio.idle();
            }
        }

        let Some(gfx) = &mut self.gfx else { return };
        let size = gfx.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        gfx.surface.resize(w, h).expect("resize surface");
        let mut buffer = gfx.surface.buffer_mut().expect("surface buffer");
        self.scene.draw(
            &mut buffer,
            Layout::new(size.width as f32, size.height as f32),
            open,
            self.press.value,
            self.active.value,
            now.duration_since(self.started).as_secs_f32(),
        );
        gfx.window.pre_present_notify();
        buffer.present().expect("present");
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let icon = Icon::from_rgba(inflate(assets::ICON), assets::ICON_SIZE, assets::ICON_SIZE).ok();
        let attrs = Window::default_attributes()
            .with_title("Popue")
            .with_inner_size(LogicalSize::new(WINDOW_SIZE, WINDOW_SIZE))
            .with_min_inner_size(LogicalSize::new(MIN_WINDOW_SIZE, MIN_WINDOW_SIZE))
            .with_window_icon(icon);
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland as _;
            attrs.with_name("popue-button", "popue-button")
        };
        let window = Rc::new(event_loop.create_window(attrs).expect("create window"));
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        window.request_redraw();
        self.gfx = Some(Gfx { window, surface });
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            if let Some(gfx) = &self.gfx {
                gfx.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(gfx) = &self.gfx {
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(position);
                if let Some(gfx) = &self.gfx {
                    let icon = if self.over_button() { CursorIcon::Pointer } else { CursorIcon::Default };
                    gfx.window.set_cursor(icon);
                }
            }
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed if self.over_button() => {
                    self.on_press();
                    self.last_frame = Instant::now();
                    if let Some(gfx) = &self.gfx {
                        gfx.window.request_redraw();
                    }
                }
                // Releasing anywhere ends the press, like a normal button.
                ElementState::Released => self.pressed = false,
                _ => {}
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Sleep until the next input event unless something is moving.
        event_loop.set_control_flow(if self.animating() {
            ControlFlow::WaitUntil(self.last_frame + FRAME)
        } else {
            ControlFlow::Wait
        });
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_decode() {
        for data in [assets::CLOSED, assets::OPEN] {
            assert_eq!(inflate(data).len(), assets::IMAGE_SIZE * assets::IMAGE_SIZE * 3);
        }
        let icon = assets::ICON_SIZE as usize;
        assert_eq!(inflate(assets::ICON).len(), icon * icon * 4);
        assert_eq!(inflate(assets::SOUND).len(), assets::SOUND_SAMPLES * 2);
        let glyphs = inflate(assets::GLYPH_MASKS);
        let last = assets::GLYPHS.last().unwrap();
        assert_eq!(glyphs.len(), last.offset + last.width * last.height);
    }

    #[test]
    fn draws_into_frame() {
        let mut app = App::new();
        let mut frame = vec![0; 300 * 300];
        let layout = Layout::new(300.0, 300.0);
        app.scene.draw(&mut frame, layout, true, 1.0, 1.0, 0.3);
        assert!(layout.button_contains(150.0, 120.0));
        assert!(!layout.button_contains(5.0, 5.0));
        assert!(frame.iter().any(|&p| p != frame[0]));
    }
}
