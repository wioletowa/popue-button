// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Cursor;

use eframe::egui;

const CLOSED_PNG: &[u8] = include_bytes!("../assets/closed.png");
const OPEN_PNG: &[u8] = include_bytes!("../assets/open.png");
const SOUND_MP3: &[u8] = include_bytes!("../assets/popue.mp3");

const TITLE: &str = "popue!";

const WINDOW_SIZE: f32 = 400.0;
/// The button fills this fraction of the window's shorter side.
const BUTTON_FRACTION: f32 = 0.8;
const MIN_WINDOW_SIZE: f32 = 200.0;

fn decode_png(bytes: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("embedded PNG is valid")
        .to_rgba8()
}

fn to_color_image(img: &image::RgbaImage) -> egui::ColorImage {
    let size = [img.width() as usize, img.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}

struct Audio {
    // Must stay alive for the output device to keep playing.
    sink: rodio::MixerDeviceSink,
    player: Option<rodio::Player>,
}

impl Audio {
    fn open() -> Option<Self> {
        match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                Some(Self { sink, player: None })
            }
            Err(err) => {
                eprintln!("audio unavailable: {err}");
                None
            }
        }
    }

    /// Stops the current sound (if any) and plays it again from the start.
    fn restart(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
        match rodio::play(self.sink.mixer(), Cursor::new(SOUND_MP3)) {
            Ok(player) => self.player = Some(player),
            Err(err) => eprintln!("failed to play sound: {err}"),
        }
    }

    fn is_playing(&self) -> bool {
        self.player.as_ref().is_some_and(|p| !p.empty())
    }
}

struct App {
    closed: egui::TextureHandle,
    open: egui::TextureHandle,
    audio: Option<Audio>,
    was_pressed: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let load = |name, bytes| {
            cc.egui_ctx.load_texture(
                name,
                to_color_image(&decode_png(bytes)),
                egui::TextureOptions::LINEAR,
            )
        };
        Self {
            closed: load("closed", CLOSED_PNG),
            open: load("open", OPEN_PNG),
            audio: Audio::open(),
            was_pressed: false,
        }
    }
}

/// Draws the title centred at `center`. While `activity` (0..=1) is above zero
/// the letters hop in a wave and the text grows and brightens.
fn paint_title(painter: &egui::Painter, center: egui::Pos2, font_size: f32, time: f32, activity: f32) {
    let ease = activity * activity * (3.0 - 2.0 * activity);
    let size = font_size * (1.0 + 0.12 * ease);
    let font = egui::FontId::proportional(size);
    let idle = egui::Color32::from_rgb(0x8d, 0x99, 0xae);
    let lit = egui::Color32::from_rgb(0xff, 0xd1, 0x66);
    let color = idle.lerp_to_gamma(lit, ease);

    let glyphs: Vec<_> = TITLE
        .chars()
        .map(|c| painter.layout_no_wrap(c.to_string(), font.clone(), color))
        .collect();
    let spacing = size * 0.04;
    let total: f32 = glyphs.iter().map(|g| g.size().x + spacing).sum::<f32>() - spacing;

    let mut x = center.x - total / 2.0;
    for (i, galley) in glyphs.into_iter().enumerate() {
        let phase = time * 9.0 - i as f32 * 0.7;
        let hop = phase.sin().max(0.0) * size * 0.35 * ease;
        let pos = egui::pos2(x, center.y - galley.size().y / 2.0 - hop);
        x += galley.size().x + spacing;
        painter.galley(pos, galley, color);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(0x2b, 0x2d, 0x42)))
            .show(ui, |ui| {
                let area = ui.max_rect();
                // Button and title together are about 1.2 button-heights tall.
                let side = (area.width() * BUTTON_FRACTION)
                    .min(area.height() * BUTTON_FRACTION / 1.2);
                let font_size = side * 0.12;
                let rect = egui::Rect::from_center_size(
                    area.center() - egui::vec2(0.0, font_size * 0.8),
                    egui::Vec2::splat(side),
                );
                let response = ui
                    .interact(rect, ui.scope_id().with("popue"), egui::Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);

                let pressed = response.is_pointer_button_down_on();
                if pressed && !self.was_pressed {
                    if let Some(audio) = &mut self.audio {
                        audio.restart();
                    }
                }
                self.was_pressed = pressed;

                let playing = self.audio.as_ref().is_some_and(Audio::is_playing);
                let active = pressed || playing;
                let texture = if active { &self.open } else { &self.closed };

                let ctx = ui.ctx().clone();
                let press_t = ctx.animate_bool_with_time(response.id.with("press"), pressed, 0.08);
                let active_t = ctx.animate_bool_with_time(response.id.with("active"), active, 0.25);

                // Slight shrink while held, so it feels like a button.
                let draw_rect = rect.shrink(rect.width() * 0.03 * press_t);
                egui::Image::new(texture)
                    .corner_radius(rect.width() * 0.075)
                    .paint_at(ui, draw_rect);

                paint_title(
                    ui.painter(),
                    egui::pos2(rect.center().x, rect.bottom() + font_size * 1.1),
                    font_size,
                    ui.input(|i| i.time) as f32,
                    active_t,
                );

                if playing || active_t > 0.0 {
                    // Keep repainting while the title moves and so the mouth
                    // closes as soon as the sound ends.
                    ctx.request_repaint();
                }
            });
    }
}

fn main() -> eframe::Result {
    let icon = decode_png(CLOSED_PNG);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Popue")
            .with_app_id("popue-button")
            .with_inner_size([WINDOW_SIZE, WINDOW_SIZE])
            .with_min_inner_size([MIN_WINDOW_SIZE, MIN_WINDOW_SIZE])
            .with_icon(egui::IconData {
                width: icon.width(),
                height: icon.height(),
                rgba: icon.into_raw(),
            }),
        ..Default::default()
    };
    eframe::run_native("Popue", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_decode() {
        let closed = decode_png(CLOSED_PNG);
        let open = decode_png(OPEN_PNG);
        assert_eq!(closed.dimensions(), open.dimensions());

        let sound = rodio::Decoder::new(Cursor::new(SOUND_MP3)).expect("mp3 decodes");
        assert!(sound.count() > 0);
    }
}
