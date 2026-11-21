//! One window for the mouse, keyboard and webcam.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use perch_core::{camera, razer, redragon};
use rune::tok::{self, Theme, TextStyle, radius, space, type_};
use rune::{Align, AppSpec, Color, Cx, Drag, El, Phase, Pixels, ScrollSpec, col, el, row, text};

struct Preview {
    pixels: Pixels,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    width: u32,
    height: u32,
    fps: u32,
    measured: Arc<std::sync::atomic::AtomicU32>,
    cap_size: Arc<std::sync::atomic::AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
    dirty: Arc<AtomicBool>,
    waker: Option<rune::Waker>,
}

impl Preview {
    fn new() -> Preview {
        Preview {
            pixels: Pixels::default(),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
            width: 640,
            height: 360,
            fps: 30,
            measured: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            cap_size: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            error: Arc::new(Mutex::new(None)),
            dirty: Arc::new(AtomicBool::new(false)),
            waker: None,
        }
    }

    fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        let stop = self.stop.clone();
        let pixels = self.pixels.clone();
        let error = self.error.clone();
        let measured = self.measured.clone();
        let cap_size = self.cap_size.clone();
        let dirty = self.dirty.clone();
        let waker = self.waker.clone();
        *error.lock().unwrap() = None;
        measured.store(0, Ordering::Relaxed);
        cap_size.store(0, Ordering::Relaxed);
        let (width, height, fps) = (self.width, self.height, self.fps);
        self.handle = Some(std::thread::spawn(move || {
            let mut cap = match perch_core::preview::Capture::open("Brio", (width, height), fps) {
                Ok(c) => c,
                Err(e) => {
                    *error.lock().unwrap() = Some(format!("{e}"));
                    dirty.store(true, Ordering::Relaxed);
                    if let Some(wake) = &waker { wake(); }
                    return;
                }
            };
            let mut warmup = 2u32;
            let mut announced = false;
            let mut shown = 0u32;
            let mut window = std::time::Instant::now();
            while !stop.load(Ordering::Relaxed) {
                match cap.frame() {
                    Ok(Some((w, h, rgba))) => {
                        cap_size.store(((w as u64) << 32) | h as u64, Ordering::Relaxed);
                        shown += 1;
                        let elapsed = window.elapsed().as_secs_f32();
                        if elapsed >= 1.0 {
                            measured.store((shown as f32 / elapsed).round() as u32, Ordering::Relaxed);
                            shown = 0;
                            window = std::time::Instant::now();
                            // Re-run the view only for the label; Pixels::set wakes the
                            // window and uploads each frame on its own.
                            dirty.store(true, Ordering::Relaxed);
                            if let Some(wake) = &waker { wake(); }
                        }
                        // The first frames are black while exposure settles.
                        if warmup > 0 {
                            warmup -= 1;
                        } else {
                            let (dw, dh, small) = downscale(w, h, rgba);
                            pixels.set(dw, dh, small);
                            if !announced {
                                announced = true;
                                dirty.store(true, Ordering::Relaxed);
                                if let Some(wake) = &waker { wake(); }
                            }
                        }
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(2)),
                    Err(e) => {
                        *error.lock().unwrap() = Some(format!("{e}"));
                        dirty.store(true, Ordering::Relaxed);
                        if let Some(wake) = &waker { wake(); }
                        return;
                    }
                }
            }
        }));
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    fn restart(&mut self) {
        self.stop();
        self.start();
    }

    fn set_mode(&mut self, width: u32, height: u32, fps: u32) {
        if (self.width, self.height, self.fps) == (width, height, fps) {
            return;
        }
        self.stop();
        self.width = width;
        self.height = height;
        self.fps = fps;
        self.start();
    }
}

const WINDOW: (f32, f32) = (1040.0, 860.0);
const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/Geist-Regular.ttf"),
    include_bytes!("../assets/Geist-Medium.ttf"),
    include_bytes!("../assets/Geist-SemiBold.ttf"),
    include_bytes!("../assets/Geist-Bold.ttf"),
    include_bytes!("../assets/GeistMono-Regular.ttf"),
    include_bytes!("../assets/GeistMono-Medium.ttf"),
];

/// Dark surfaces with a blue-violet accent; every colour goes through OKLab
/// tokens so text and control contrast stay predictable.
fn ui_theme() -> Theme {
    let mut t = tok::theme(true);
    t.bg = swatch([10, 11, 14]);
    t.card = swatch([19, 21, 26]);
    t.card_border = Color::WHITE.alpha(0.055);
    t.text = swatch([244, 245, 247]);
    t.text_secondary = swatch([154, 160, 170]);
    t.separator = Color::WHITE.alpha(0.06);
    t.control_track = Color::WHITE.alpha(0.055);
    t.control_off = Color::WHITE.alpha(0.10);
    t.indicator = swatch([40, 43, 51]);
    t.knob = Color::WHITE;
    t.hover = Color::WHITE.alpha(0.03);
    t.press = Color::WHITE.alpha(0.055);
    t.shadow = Color::BLACK.alpha(0.35);
    t.shadow_soft = Color::BLACK.alpha(0.22);
    t.edge = Color::BLACK.alpha(0.28);
    t.knob_shadow = Color::BLACK.alpha(0.28);
    t
}

fn brand(p: Paint) -> El {
    let mut m = el().key("mark").size(26.0, 26.0).radius(8.0).fill(p.accent);
    for (y, x) in [(7.0f32, 12.0f32), (12.0, 9.0), (17.0, 15.0)] {
        m = m
            .child(
                el()
                    .size(14.0, 2.0)
                    .absolute(6.0, y)
                    .radius(1.0)
                    .fill(Color::WHITE.alpha(0.5)),
            )
            .child(
                el()
                    .size(4.5, 4.5)
                    .absolute(x, y - 1.25)
                    .radius(2.25)
                    .fill(Color::WHITE),
            );
    }
    m
}
fn grid_mark(p: Paint) -> El {
    let mut m = el().size(18.0, 18.0).radius(5.0).fill(p.accent.alpha(0.15));
    for (x, y) in [(4.0f32, 4.0f32), (10.0, 4.0), (4.0, 10.0), (10.0, 10.0)] {
        m = m.child(el().size(4.0, 4.0).absolute(x, y).radius(1.5).fill(p.accent));
    }
    m
}

const SLIDER_W: f32 = 260.0;
const HERO: TextStyle = TextStyle::new(28.0, 600.0);
const CARD_TITLE: TextStyle = TextStyle::new(16.0, 600.0);

const SWATCHES: [(&str, [u8; 3]); 9] = [
    ("red", [255, 0, 0]),
    ("green", [0, 255, 0]),
    ("blue", [0, 0, 255]),
    ("yellow", [255, 255, 0]),
    ("cyan", [0, 255, 255]),
    ("magenta", [255, 0, 255]),
    ("orange", [255, 140, 0]),
    ("purple", [160, 32, 240]),
    ("white", [255, 255, 255]),
];

fn srgb(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn swatch(rgb: [u8; 3]) -> Color {
    Color { r: srgb(rgb[0]), g: srgb(rgb[1]), b: srgb(rgb[2]), a: 1.0 }
}

fn same(a: [u8; 3], b: [u8; 3]) -> bool {
    a == b
}

fn cam_label(id: camera::ControlId) -> &'static str {
    match id {
        camera::ControlId::Brightness => "Brightness",
        camera::ControlId::Contrast => "Contrast",
        camera::ControlId::Saturation => "Saturation",
        camera::ControlId::Sharpness => "Sharpness",
        camera::ControlId::Gain => "Gain",
        camera::ControlId::BacklightCompensation => "Backlight compensation",
        camera::ControlId::WhiteBalance => "White balance",
        camera::ControlId::Exposure => "Exposure",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Home,
    Mouse,
    Keyboard,
    Camera,
}

struct App {
    tab: Tab,
    status: String,
    mouse: Option<razer::Mouse>,
    dpi: u16,
    polling: u16,
    mouse_brightness: u8,
    mouse_effect: razer::Effect,
    mouse_color: [u8; 3],
    keyboard: Option<redragon::Keyboard>,
    key_effect: redragon::Effect,
    key_color: [u8; 3],
    key_speed: u8,
    key_brightness: u8,
    camera: Option<camera::Camera>,
    cam: Vec<camera::Control>,
    preview: Preview,
    backdrop: Pixels,
    waker: Option<rune::Waker>,
    results: Arc<Mutex<Vec<String>>>,
}

impl App {
    /// Tab changes own the preview lifecycle: it only runs while the camera is
    /// on screen, so Home and the other panels keep the window cheap.
    fn go(&mut self, tab: Tab) {
        if self.tab == tab {
            return;
        }
        if tab == Tab::Camera {
            self.preview.start();
        } else {
            self.preview.stop();
        }
        self.tab = tab;
    }

    fn rescan(&mut self) {
        self.mouse = razer::Mouse::open().ok();
        match &self.mouse {
            Some(m) => match m.state() {
                Ok(state) => {
                    self.dpi = state.dpi_x;
                    self.polling = state.polling;
                    self.mouse_brightness = (state.brightness as u16 * 100 / 255) as u8;
                    self.mouse_effect = state.effect;
                    self.mouse_color = [255, 255, 255];
                }
                Err(e) => self.status = format!("mouse: {e}"),
            },
            None => self.status = "mouse: not connected".into(),
        }

        self.keyboard = redragon::Keyboard::open().ok();
        if self.keyboard.is_none() {
            self.status = "keyboard: not connected".into();
        }

        self.camera = camera::Camera::open().ok();
        match &self.camera {
            Some(c) => self.cam = c.controls(),
            None => self.cam.clear(),
        }

        let connected = [self.mouse.is_some(), self.keyboard.is_some(), self.camera.is_some()]
            .into_iter()
            .filter(|x| *x)
            .count();
        if self.tab == Tab::Camera {
            if self.camera.is_some() {
                self.preview.restart();
            } else {
                self.preview.stop();
            }
        }
        self.status = format!("{connected} of 3 devices connected");
    }

    /// Hardware work leaves the UI thread: feature reports take tens of
    /// milliseconds, and a blocked handler is exactly what makes the window
    /// stutter.
    fn spawn(&self, job: impl FnOnce() -> perch_core::Result<String> + Send + 'static) {
        let results = self.results.clone();
        let waker = self.waker.clone();
        std::thread::spawn(move || {
            let msg = match job() {
                Ok(m) => m,
                Err(e) => format!("{e}"),
            };
            results.lock().unwrap().push(msg);
            if let Some(wake) = waker {
                wake();
            }
        });
    }

    fn commit_dpi(&mut self) {
        let dpi = self.dpi;
        self.status = format!("DPI {dpi}…");
        self.spawn(move || {
            razer::Mouse::open()?.set_dpi_persistent(dpi)?;
            Ok(format!("DPI {dpi} applied"))
        });
    }

    fn commit_polling(&mut self) {
        let hz = self.polling;
        self.status = format!("polling {hz} Hz…");
        self.spawn(move || {
            razer::Mouse::open()?.set_polling(hz)?;
            Ok(format!("polling {hz} Hz"))
        });
    }

    fn commit_mouse_brightness(&mut self) {
        let pct = self.mouse_brightness;
        self.status = format!("brightness {pct}%…");
        self.spawn(move || {
            razer::Mouse::open()?.set_brightness(pct)?;
            Ok(format!("logo brightness {pct}%"))
        });
    }

    fn commit_mouse_effect(&mut self) {
        let effect = match self.mouse_effect {
            razer::Effect::Off => razer::Effect::Off,
            razer::Effect::Breathing(_) => razer::Effect::Breathing([255, 255, 255]),
            _ => razer::Effect::Static([255, 255, 255]),
        };
        self.mouse_effect = effect;
        self.status = format!("logo {}…", effect.spec());
        self.spawn(move || {
            razer::Mouse::open()?.set_effect(effect)?;
            Ok(format!("logo {}", effect.spec()))
        });
    }

    fn commit_keyboard(&mut self) {
        let (effect, color, speed, brightness) = (
            self.key_effect,
            if self.key_effect.uses_color() { Some(self.key_color) } else { None },
            self.key_speed,
            self.key_brightness,
        );
        self.status = format!("{}…", effect.name());
        self.spawn(move || {
            redragon::Keyboard::open()?.apply(effect, color, Some(speed), Some(brightness))?;
            Ok(format!("{} applied", effect.name()))
        });
    }

    fn commit_cam(&mut self, id: camera::ControlId, value: i32) {
        let res = match &self.camera {
            Some(c) => c.set(id, value),
            None => return,
        };
        match res {
            Ok(()) => {
                if let Some(c) = self.cam.iter_mut().find(|c| c.id == id) {
                    c.value = value;
                    c.auto = Some(false);
                }
                self.status = format!("{} = {value}", id.name());
            }
            Err(e) => self.status = format!("{}: {e}", id.name()),
        }
    }

    fn commit_cam_auto(&mut self, id: camera::ControlId, on: bool) {
        let res = match &self.camera {
            Some(c) => c.set_auto(id, on),
            None => return,
        };
        match res {
            Ok(()) => {
                if let Some(c) = self.cam.iter_mut().find(|c| c.id == id) {
                    c.auto = Some(on);
                }
                self.status = format!("{}: {}", id.name(), if on { "auto" } else { "manual" });
            }
            Err(e) => self.status = format!("{}: {e}", id.name()),
        }
    }

    fn cam_control(&self, id: camera::ControlId) -> Option<&camera::Control> {
        self.cam.iter().find(|c| c.id == id)
    }
}

/// The dithered sky from the background set, pre-darkened and packed as raw
/// RGBA so the app needs no image decoder at runtime.
fn backdrop() -> Pixels {
    let bytes = include_bytes!("../assets/backdrop.bin");
    let w = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let pixels = Pixels::default();
    pixels.set(w, h, bytes[8..].to_vec());
    pixels
}

/// The preview never needs more than ~640 px of width; uploading a full
/// 1080p RGBA frame every frame is what makes the window feel heavy.
fn downscale(w: u32, h: u32, rgba: Vec<u8>) -> (u32, u32, Vec<u8>) {
    const MAX: u32 = 640;
    if w <= MAX {
        return (w, h, rgba);
    }
    let nw = MAX;
    let nh = ((h as u64 * MAX as u64 / w as u64) as u32).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let sy = (y as u64 * h as u64 / nh as u64) as usize;
        for x in 0..nw {
            let sx = (x as u64 * w as u64 / nw as u64) as usize;
            let si = (sy * w as usize + sx) * 4;
            let di = (y as usize * nw as usize + x as usize) * 4;
            out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
        }
    }
    (nw, nh, out)
}

fn init() -> App {
    let mut app = App {
        tab: Tab::Home,
        status: String::new(),
        mouse: None,
        dpi: 1600,
        polling: 1000,
        mouse_brightness: 100,
        mouse_effect: razer::Effect::Off,
        mouse_color: [255, 255, 255],
        keyboard: None,
        key_effect: redragon::Effect::ALL[0],
        key_color: [255, 0, 0],
        key_speed: 4,
        key_brightness: 4,
        camera: None,
        cam: Vec::new(),
        preview: Preview::new(),
        backdrop: backdrop(),
        waker: None,
        results: Arc::new(Mutex::new(Vec::new())),
    };
    app.rescan();
    app
}

#[derive(Clone, Copy)]
struct Paint {
    t: Theme,
    accent: Color,
    hair: f32,
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = ui_theme();
    let p = Paint { t, accent: swatch([91, 108, 255]), hair: 1.0 / cx.env().scale };

    col()
        .key("window")
        .role("window")
        .fill(t.bg)
        .child(
            el()
                .key("backdrop")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .pixels(&s.backdrop)
                .opacity(0.85)
                .a11y_hidden(),
        )
        .child(
            el()
                .key("scrim")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .fill(t.bg.alpha(0.45))
                .a11y_hidden(),
        )
        .child(titlebar(s, p))
        .child(el().key("rule").h(p.hair).fill(p.t.card_border))
        .child(
            row()
                .key("body")
                .grow(1.0)
                .min_h(0.0)
                .child(sidebar(s, p))
                .child(panel(s, p)),
        )
}

fn titlebar(s: &App, p: Paint) -> El {
    row()
        .key("titlebar")
        .h(64.0)
        .w_full()
        .px(22.0)
        .center_y()
        .gap(12.0)
        .child(brand(p))
        .child(
            text("Perch")
                .key("title")
                .font(type_::TITLE)
                .color(p.t.text),
        )
        .child(el().grow(1.0))
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    el()
                        .size(6.0, 6.0)
                        .radius(3.0)
                        .fill(if s.status.contains("not") || s.status.contains("0 of") {
                            swatch([255, 159, 10])
                        } else {
                            swatch([52, 199, 89])
                        }),
                )
                .child(
                    text(s.status.as_str())
                        .key("status")
                        .font(type_::LABEL)
                        .color(p.t.text_secondary),
                ),
        )
}

fn sidebar(s: &App, p: Paint) -> El {
    let mut cards = col().gap(6.0);
    for (tab, name, detail, connected) in [
        (
            Tab::Mouse,
            "DeathAdder Essential".to_string(),
            "DPI · polling · logo",
            s.mouse.is_some(),
        ),
        (
            Tab::Keyboard,
            "K616 Fizz Pro".to_string(),
            "effects · colour · speed",
            s.keyboard.is_some(),
        ),
        (
            Tab::Camera,
            s.camera.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Brio 100".into()),
            "image · exposure · balance",
            s.camera.is_some(),
        ),
    ] {
        let selected = s.tab == tab;
        cards = cards.child(
            el()
                .key(name.clone())
                .role("tab")
                .label(name.clone())
                .checked(selected)
                .focusable(true)
                .h(64.0)
                .px(14.0)
                .radius(radius::LG)
                .fill(if selected { p.t.card } else { Color::TRANSPARENT })
                .hover(p.t.hover)
                .press(p.t.press)
                .on_press_tx(move |s: &mut App, tx| {
                    tx.instant();
                    s.go(tab)
                })
                .child(
                    row()
                        .gap(space::S3)
                        .center_y()
                        .child(
                            el()
                                .size(7.0, 7.0)
                                .radius(3.5)
                                .fill(if connected {
                                    swatch([52, 199, 89])
                                } else {
                                    p.t.text_secondary.alpha(0.35)
                                }),
                        )
                        .child(
                            col()
                                .gap(2.0)
                                .child(
                                    text(name)
                                        .key("name")
                                        .font(type_::BODY)
                                        .color(if selected { p.t.text } else { p.t.text_secondary }),
                                )
                                .child(
                                    text(detail)
                                        .key("detail")
                                        .font(type_::SECTION)
                                        .color(p.t.text_secondary.alpha(0.8)),
                                ),
                        ),
                ),
        );
    }

    let home_selected = s.tab == Tab::Home;
    let home = el()
        .key("home-nav")
        .role("tab")
        .label("Home")
        .checked(home_selected)
        .focusable(true)
        .h(44.0)
        .px(12.0)
        .radius(radius::LG)
        .fill(if home_selected { p.t.card } else { Color::TRANSPARENT })
        .hover(p.t.hover)
        .press(p.t.press)
        .on_press_tx(|s: &mut App, tx| {
            tx.instant();
            s.go(Tab::Home)
        })
        .child(
            row()
                .gap(space::S3)
                .center_y()
                .child(grid_mark(p))
                .child(
                    text("Home")
                        .key("name")
                        .font(type_::BODY)
                        .color(if home_selected { p.t.text } else { p.t.text_secondary }),
                ),
        );

    col()
        .key("sidebar")
        .w(260.0)
        .h_full()
        .px(14.0)
        .py(18.0)
        .gap(14.0)
        .fill(p.t.bg.alpha(0.42))
        .border(p.hair, p.t.card_border)
        .child(
            text("OVERVIEW")
                .key("overview")
                .font(type_::SECTION)
                .color(p.t.text_secondary)
                .ml(space::S2),
        )
        .child(home)
        .child(
            text("DEVICES")
                .key("devices")
                .font(type_::SECTION)
                .color(p.t.text_secondary)
                .ml(space::S2),
        )
        .child(cards)
        .child(el().grow(1.0))
        .child(
            el()
                .key("rescan")
                .role("button")
                .label("Rescan devices")
                .focusable(true)
                .h(36.0)
                .center()
                .radius(radius::MD)
                .fill(p.t.control_track)
                .hover(p.t.hover)
                .press(p.t.press)
                .on_press_tx(|s: &mut App, tx| {
                    tx.instant();
                    s.rescan()
                })
                .child(text("Rescan devices").key("label").font(type_::LABEL).color(p.t.text)),
        )
}

fn panel(s: &App, p: Paint) -> El {
    let mut body = col()
        .key("panel")
        .grow(1.0)
        .h_full()
        .min_h(0.0)
        .px(26.0)
        .pt(18.0)
        .pb(10.0)
        .gap(18.0)
        .scroll_edge_fade(16.0)
        .scroll(ScrollSpec { stick_bottom: false });
    body = match s.tab {
        Tab::Home => body
            .child(home_hero(p))
            .child(home_cards(s, p))
            .child(el().grow(1.0))
            .child(
                text("Settings are written to the device, not kept in a config file.")
                    .key("home-note")
                    .font(type_::SECTION)
                    .color(p.t.text_secondary.alpha(0.7)),
            ),
        Tab::Mouse => body
            .child(section(
                p,
                "performance",
                "PERFORMANCE",
                vec![
                    dpi_row(s, p),
                    polling_row(s, p),
                ],
            ))
            .child(section(
                p,
                "lighting",
                "LOGO LIGHTING",
                vec![
                    mouse_effect_row(s, p),
                    mouse_brightness_row(s, p),

                ],
            )),
        Tab::Keyboard => body
            .child(section(p, "effect", "EFFECT", vec![effect_grid(s, p)]))
            .child(section(
                p,
                "colour",
                "COLOUR",
                vec![keyboard_swatches(s, p), rgb_editor(s, p)],
            ))
            .child(section(
                p,
                "tuning",
                "SPEED & BRIGHTNESS",
                vec![key_speed_row(s, p), key_brightness_row(s, p)],
            )),
        Tab::Camera => camera_panel(s, p),
    };
    body
}

fn home_hero(p: Paint) -> El {
    col()
        .key("hero")
        .gap(7.0)
        .pt(10.0)
        .child(
            text("SETUP")
                .font(type_::SECTION.tracking(0.9))
                .color(p.t.text_secondary),
        )
        .child(
            text("Your gear, at a glance.")
                .font(HERO.tracking(-0.2))
                .color(p.t.text),
        )
        .child(
            text("Pick a device to tune it. Every change is applied as you make it.")
                .font(type_::BODY)
                .color(p.t.text_secondary),
        )
}

fn home_cards(s: &App, p: Paint) -> El {
    let mouse_stats = if s.mouse.is_some() {
        format!("{} DPI · {} Hz", s.dpi, s.polling)
    } else {
        "—".into()
    };
    let key_stats = if s.keyboard.is_some() {
        format!("{} · speed {}/4", s.key_effect.name(), s.key_speed)
    } else {
        "—".into()
    };
    let cam_stats = if s.camera.is_some() {
        let auto = s
            .cam_control(camera::ControlId::Exposure)
            .and_then(|c| c.auto)
            .unwrap_or(false);
        format!(
            "{} controls · {}",
            s.cam.len(),
            if auto { "auto exposure" } else { "manual exposure" }
        )
    } else {
        "—".into()
    };
    let cam_name = s
        .camera
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "Brio 100".into());

    row()
        .key("home-cards")
        .gap(12.0)
        .w_full()
        .child(home_card(
            p,
            Tab::Mouse,
            "home-mouse",
            "MOUSE",
            "DeathAdder Essential".into(),
            s.mouse.is_some(),
            mouse_stats,
        ))
        .child(home_card(
            p,
            Tab::Keyboard,
            "home-keyboard",
            "KEYBOARD",
            "K616 Fizz Pro".into(),
            s.keyboard.is_some(),
            key_stats,
        ))
        .child(home_card(
            p,
            Tab::Camera,
            "home-camera",
            "CAMERA",
            cam_name,
            s.camera.is_some(),
            cam_stats,
        ))
}

fn home_card(
    p: Paint,
    tab: Tab,
    key: &str,
    kind: &str,
    name: String,
    connected: bool,
    stats: String,
) -> El {
    let action = match tab {
        Tab::Mouse => "Open mouse settings",
        Tab::Keyboard => "Open keyboard settings",
        Tab::Camera => "Open camera settings",
        Tab::Home => "Home",
    };
    col()
        .key(key)
        .role("button")
        .label(action)
        .focusable(true)
        .grow(1.0)
        .h(188.0)
        .padding(16.0)
        .gap(6.0)
        .radius(radius::LG)
        .fill(p.t.card)
        .border(p.hair, p.t.card_border)
        .hover(p.t.hover)
        .press(p.t.press)
        .on_press_tx(move |s: &mut App, tx| {
            tx.instant();
            s.go(tab)
        })
        .child(
            text(kind)
                .font(type_::SECTION.tracking(0.9))
                .color(p.t.text_secondary),
        )
        .child(text(name).font(CARD_TITLE).color(p.t.text))
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(
                    el()
                        .size(6.0, 6.0)
                        .radius(3.0)
                        .fill(if connected {
                            swatch([52, 199, 89])
                        } else {
                            p.t.text_secondary.alpha(0.35)
                        }),
                )
                .child(
                    text(if connected { "Connected" } else { "Not found" })
                        .font(type_::LABEL)
                        .color(p.t.text_secondary),
                ),
        )
        .child(
            text(stats)
                .font(type_::LABEL)
                .color(p.t.text_secondary.alpha(0.85)),
        )
        .child(el().grow(1.0))
        .child(text(action).font(type_::LABEL).color(p.accent))
}



fn preview_panel(s: &App, p: Paint) -> El {
    let cap = s.preview.cap_size.load(Ordering::Relaxed);
    let (cw, ch) = if cap != 0 {
        ((cap >> 32) as u32, cap as u32)
    } else {
        (s.preview.width, s.preview.height)
    };
    let fps = s.preview.measured.load(Ordering::Relaxed);
    let (dw, _) = s.preview.pixels.size();
    let error = s.preview.error.lock().ok().and_then(|g| g.clone());
    let res = match s.preview.width {
        640 => 0,
        1280 => 1,
        _ => 2,
    };
    let fr = match s.preview.fps {
        15 => 0,
        30 => 1,
        _ => 2,
    };

    let preview = el()
        .key("preview")
        .w(360.0)
        .h(203.0)
        .radius(radius::MD)
        .fill(p.t.control_track)
        .pixels(&s.preview.pixels)
        .a11y_hidden();

    let mut right = col()
        .grow(1.0)
        .min_h(0.0)
        .gap(6.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(
                    text(if dw == 0 {
                        "Starting camera…".to_string()
                    } else {
                        format!("{cw}×{ch}")
                    })
                    .font(type_::LABEL)
                    .color(p.t.text),
                )
                .child(
                    text(format!("{fps} fps"))
                        .font(type_::MONO_CAPTION)
                        .color(p.t.text_secondary),
                ),
        )
        .child(control_row(
            p,
            "resolution",
            "Resolution",
            segmented(p, 180.0, &["360p", "720p", "1080p"], res, |s, i| {
                let (w, h) = [(640u32, 360u32), (1280, 720), (1920, 1080)][i];
                let fps = s.preview.fps;
                s.preview.set_mode(w, h, fps);
            }),
        ))
        .child(control_row(
            p,
            "frame-rate",
            "Frame rate",
            segmented(p, 150.0, &["15", "30", "60"], fr, |s, i| {
                let fps = [15u32, 30, 60][i];
                let (w, h) = (s.preview.width, s.preview.height);
                s.preview.set_mode(w, h, fps);
            }),
        ));

    if let Some(err) = error {
        right = right.child(
            text(err)
                .font(type_::SECTION)
                .color(swatch([255, 159, 10])),
        );
    } else if dw > 0 && fps < 20 {
        right = right.child(
            text("Low light makes the camera lower its frame rate.")
                .font(type_::SECTION)
                .color(p.t.text_secondary),
        );
    }

    row()
        .padding(12.0)
        .gap(16.0)
        .child(preview)
        .child(right)
}

fn camera_panel(s: &App, p: Paint) -> El {
    let mut image = Vec::new();
    for id in [
        camera::ControlId::Brightness,
        camera::ControlId::Contrast,
        camera::ControlId::Saturation,
        camera::ControlId::Sharpness,
        camera::ControlId::Gain,
    ] {
        if let Some(c) = s.cam_control(id) {
            image.push(cam_slider_row(p, c));
        }
    }
    if let Some(c) = s.cam_control(camera::ControlId::BacklightCompensation) {
        image.push(cam_toggle_row(p, c));
    }

    let mut body = col()
        .key("panel")
        .grow(1.0)
        .h_full()
        .min_h(0.0)
        .px(26.0)
        .pt(18.0)
        .pb(10.0)
        .gap(18.0)
        .scroll_edge_fade(16.0)
        .scroll(ScrollSpec { stick_bottom: false });

    body = body.child(section(p, "preview", "PREVIEW", vec![preview_panel(s, p)]));

    if image.is_empty() {
        body = body.child(section(
            p,
            "no-camera",
            "CAMERA",
            vec![label_row(p, "Not connected", "plug the webcam in and press Rescan")],
        ));
        return body;
    }
    body = body.child(section(p, "image", "IMAGE", image));

    let mut exposure = Vec::new();
    if let Some(c) = s.cam_control(camera::ControlId::Exposure) {
        if c.supports_auto {
            exposure.push(cam_auto_row(p, c));
        }
        exposure.push(cam_slider_row(p, c));
    }
    if !exposure.is_empty() {
        body = body.child(section(p, "exposure", "EXPOSURE", exposure));
    }

    let mut balance = Vec::new();
    if let Some(c) = s.cam_control(camera::ControlId::WhiteBalance) {
        if c.supports_auto {
            balance.push(cam_auto_row(p, c));
        }
        balance.push(cam_slider_row(p, c));
    }
    if !balance.is_empty() {
        body = body.child(section(p, "white-balance", "WHITE BALANCE", balance));
    }
    body.child(el().h(14.0))
}

fn section(p: Paint, key: &str, title: &str, rows: Vec<El>) -> El {
    let n = rows.len();
    let mut card = col()
        .key("card")
        .padding(space::S1)
        .fill(p.t.card)
        .radius(radius::LG)
        .border(p.hair, p.t.card_border)
        .shadow(1.0, 2.0, p.t.shadow)
        .shadow(8.0, 24.0, p.t.shadow_soft);
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(r);
        if i + 1 < n {
            card = card
                .child(el().key(format!("sep{i}")).h(p.hair).ml(space::S3).mr(space::S3).fill(p.t.separator).a11y_hidden());
        }
    }
    col()
        .key(key)
        .gap(space::S2)
        .child(text(title).key("header").font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
        .child(card)
}

fn label_row(p: Paint, label: &str, value: &str) -> El {
    row()
        .h(52.0)
        .px(space::S3)
        .center_y()
        .justify_between()
        .child(text(label).font(type_::BODY).color(p.t.text))
        .child(text(value).font(type_::LABEL).color(p.t.text_secondary))
}

fn dpi_row(s: &App, p: Paint) -> El {
    let norm = ((s.dpi as f32 - 200.0) / 6200.0).clamp(0.0, 1.0);
    slider_row(
        p,
        "dpi",
        "DPI",
        norm,
        format!("{}", s.dpi),
        set_dpi_norm,
        |s: &mut App| s.commit_dpi(),
    )
}

fn polling_row(s: &App, p: Paint) -> El {
    let idx = match s.polling {
        125 => 0,
        500 => 1,
        _ => 2,
    };
    control_row(
        p,
        "polling",
        "Polling rate",
        segmented(p, 190.0, &["125", "500", "1000"], idx, |s, i| {
            s.polling = [125, 500, 1000][i];
            s.commit_polling();
        }),
    )
}

fn mouse_effect_row(s: &App, p: Paint) -> El {
    let idx = match s.mouse_effect {
        razer::Effect::Off => 0,
        razer::Effect::Static(_) => 1,
        razer::Effect::Breathing(_) => 2,
    };
    control_row(
        p,
        "mouse-effect",
        "Effect",
        segmented(p, 210.0, &["Off", "Static", "Breathing"], idx, |s, i| {
            s.mouse_effect = match i {
                0 => razer::Effect::Off,
                1 => razer::Effect::Static([255, 255, 255]),
                _ => razer::Effect::Breathing([255, 255, 255]),
            };
            s.commit_mouse_effect();
        }),
    )
}

fn mouse_brightness_row(s: &App, p: Paint) -> El {
    slider_row(
        p,
        "mouse-brightness",
        "Brightness",
        s.mouse_brightness as f32 / 100.0,
        format!("{}%", s.mouse_brightness),
        |s, v| s.mouse_brightness = (v * 100.0).round() as u8,
        |s: &mut App| s.commit_mouse_brightness(),
    )
}

fn effect_grid(s: &App, p: Paint) -> El {
    let mut grid = col().gap(6.0).px(space::S2).py(space::S2);
    for chunk in redragon::Effect::ALL.chunks(5) {
        let mut r = row().gap(6.0);
        for e in chunk {
            let selected = s.key_effect == *e;
            r = r.child(
                el()
                    .key(e.name())
                    .role("radio")
                    .label(e.name())
                    .checked(selected)
                    .focusable(true)
                    .grow(1.0)
                    .h(44.0)
                    .center()
                    .radius(radius::SM)
                    .fill(if selected { p.accent.alpha(0.18) } else { p.t.control_track })
                    .border(if selected { p.hair } else { 0.0 }, p.accent)
                    .hover(p.t.hover)
                    .press(p.t.press)
                    .on_press_tx(move |s: &mut App, tx| {
                        tx.instant();
                        s.key_effect = *e;
                        s.commit_keyboard();
                    })
                    .child(
                        text(e.name())
                            .font(type_::LABEL)
                            .color(if selected { p.t.text } else { p.t.text_secondary }),
                    ),
            );
        }
        grid = grid.child(r);
    }
    grid
}

fn keyboard_swatches(s: &App, p: Paint) -> El {
    let mut body = col().gap(4.0).px(space::S2).py(space::S2);
    body = body.child(swatch_row(p, "key-colour", "Colour", s.key_color, |s, c| {
        s.key_color = c;
        if s.key_effect.uses_color() {
            s.commit_keyboard();
        }
    }));
    if !s.key_effect.uses_color() {
        body = body.child(
            text(format!("{} uses the firmware's own colours", s.key_effect.name()))
                .font(type_::SECTION)
                .color(p.t.text_secondary)
                .ml(space::S2),
        );
    }
    body
}

fn key_speed_row(s: &App, p: Paint) -> El {
    control_row(
        p,
        "key-speed",
        "Speed",
        segmented(p, 190.0, &["1", "2", "3", "4"], (s.key_speed.clamp(1, 4) - 1) as usize, |s, i| {
            s.key_speed = (i + 1) as u8;
            s.commit_keyboard();
        }),
    )
}

fn key_brightness_row(s: &App, p: Paint) -> El {
    control_row(
        p,
        "key-brightness",
        "Brightness",
        segmented(p, 190.0, &["1", "2", "3", "4"], (s.key_brightness.clamp(1, 4) - 1) as usize, |s, i| {
            s.key_brightness = (i + 1) as u8;
            s.commit_keyboard();
        }),
    )
}

fn cam_slider_row(p: Paint, c: &camera::Control) -> El {
    let span = (c.max - c.min).max(1) as f32;
    let norm = ((c.value - c.min) as f32 / span).clamp(0.0, 1.0);
    let id = c.id;
    let (min, max) = (c.min, c.max);
    let set = move |s: &mut App, v: f32| {
        let value = min + (v * span).round() as i32;
        if let Some(c) = s.cam.iter_mut().find(|c| c.id == id) {
            c.value = value;
        }
    };
    let commit = move |s: &mut App| {
        let value = s.cam.iter().find(|c| c.id == id).map(|c| c.value).unwrap_or(min);
        s.commit_cam(id, value);
    };
    let _ = max;
    slider_row(p, c.id.name(), cam_label(c.id), norm, format!("{}", c.value), set, commit)
}

fn cam_auto_row(p: Paint, c: &camera::Control) -> El {
    let id = c.id;
    let on = c.auto.unwrap_or(false);
    control_row(
        p,
        "auto",
        "Automatic mode",
        el()
            .key("switch")
            .role("switch")
            .label("Automatic")
            .checked(on)
            .focusable(true)
            .child(toggle(p, on))
            .on_press_tx(move |s: &mut App, tx| {
                tx.instant();
                let now = s.cam.iter().find(|x| x.id == id).and_then(|x| x.auto).unwrap_or(false);
                s.commit_cam_auto(id, !now);
            }),
    )
}

fn cam_toggle_row(p: Paint, c: &camera::Control) -> El {
    let id = c.id;
    let on = c.value != 0;
    control_row(
        p,
        c.id.name(),
        cam_label(c.id),
        el()
            .key("switch")
            .role("switch")
            .label(c.id.name())
            .checked(on)
            .focusable(true)
            .child(toggle(p, on))
            .on_press_tx(move |s: &mut App, tx| {
                tx.instant();
                let now = s.cam.iter().find(|x| x.id == id).map(|x| x.value != 0).unwrap_or(false);
                s.commit_cam(id, if now { 0 } else { 1 });
            }),
    )
}

fn control_row(p: Paint, key: &str, label: &str, control: El) -> El {
    row()
        .key(key)
        .h(52.0)
        .px(space::S3)
        .center_y()
        .justify_between()
        .child(text(label).font(type_::BODY).color(p.t.text))
        .child(control)
}


fn rgb_editor(s: &App, p: Paint) -> El {
    let color = s.key_color;
    let mut body = col().gap(0.0);
    for (i, (label, key)) in [("Red", "r"), ("Green", "g"), ("Blue", "b")].into_iter().enumerate() {
        let value = color[i] as f32 / 255.0;
        let readout = format!("{}", color[i]);
        let set = move |s: &mut App, v: f32| {
            s.key_color[i] = (v * 255.0).round() as u8;
        };
        let commit = move |s: &mut App| {
            if s.key_effect.uses_color() {
                s.commit_keyboard();
            }
        };
        body = body.child(slider_row(p, key, label, value, readout, set, commit));
    }
    body = body.child(
        row()
            .h(28.0)
            .px(space::S3)
            .justify_between()
            .child(text("Hex").font(type_::LABEL).color(p.t.text_secondary))
            .child(
                text(format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]))
                    .font(type_::MONO_CAPTION)
                    .color(p.t.text),
            ),
    );
    body
}
fn slider_row<F, G>(
    p: Paint,
    key: &str,
    label: &str,
    norm: f32,
    readout: String,
    on_set: F,
    on_commit: G,
) -> El
where
    F: Fn(&mut App, f32) + 'static,
    G: Fn(&mut App) + 'static,
{
    let on_set = std::rc::Rc::new(on_set);
    let on_commit = std::rc::Rc::new(on_commit);
    let (step_set, step_commit) = (on_set.clone(), on_commit.clone());
    let (w, h, k, bar) = (SLIDER_W, 40.0, 18.0, 4.0);
    let travel = w - k;
    let to_value = move |x: f32| ((x - k / 2.0) / travel).clamp(0.0, 1.0);
    row()
        .key(key)
        .h(52.0)
        .px(space::S3)
        .center_y()
        .justify_between()
        .child(text(label).font(type_::BODY).color(p.t.text))
        .child(
            row()
                .gap(space::S3)
                .center_y()
                .child(
                    el()
                        .key(format!("{key}-handle"))
                        .role("slider")
                        .label(label)
                        .range_value(f64::from(norm), 0.0, 1.0)
                        .range_step(0.01)
                        .a11y_value(readout.clone())
                        .on_step(move |s: &mut App, steps, tx| {
                            tx.instant();
                            let v = (norm + steps as f32 * 0.01).clamp(0.0, 1.0);
                            step_set(s, v);
                            step_commit(s);
                        })
                        .size(w, h)
                        .on_drag(move |s: &mut App, d: &mut Drag, tx| match d.phase {
                            Phase::Down => {
                                tx.instant();
                                d.data = norm;
                                let knob = k / 2.0 + norm * travel;
                                if (d.x - knob).abs() > k / 2.0 + 2.0 {
                                    (*on_set)(s, to_value(d.x));
                                }
                            }
                            Phase::Move => {
                                let knob = k / 2.0 + d.data * travel;
                                let grab =
                                    if (d.start_x - knob).abs() <= k / 2.0 + 2.0 { d.start_x - knob } else { 0.0 };
                                (*on_set)(s, to_value(d.x - grab));
                                tx.instant();
                            }
                            Phase::Up => {
                                tx.instant();
                                if d.cancelled() {
                                    (*on_set)(s, d.data);
                                } else {
                                    (*on_commit)(s);
                                }
                            }
                        })
                        .child(
                            el()
                                .key("track")
                                .size(w, bar)
                                .absolute(0.0, (h - bar) / 2.0)
                                .radius(bar / 2.0)
                                .fill(p.t.control_off)
                                .a11y_hidden(),
                        )
                        .child(
                            el()
                                .key("fill")
                                .size(k / 2.0 + norm * travel, bar)
                                .absolute(0.0, (h - bar) / 2.0)
                                .radius(bar / 2.0)
                                .fill(p.accent)
                                .a11y_hidden(),
                        )
                        .child(
                            el()
                                .key("knob")
                                .size(k, k)
                                .absolute(norm * travel, (h - k) / 2.0)
                                .radius(k / 2.0)
                                .fill(p.t.knob)
                                .shadow(1.0, 3.0, p.t.knob_shadow)
                                .shadow(0.0, 0.5, p.t.edge)
                                .a11y_hidden(),
                        ),
                )
                .child(
                    text(readout)
                        .key("value")
                        .w(56.0)
                        .font(type_::MONO_CAPTION)
                        .color(p.t.text_secondary)
                        .align(Align::End)
                        .a11y_hidden(),
                ),
        )
}

fn segmented(
    p: Paint,
    w: f32,
    items: &[&'static str],
    selected: usize,
    on: fn(&mut App, usize),
) -> El {
    let (h, pad) = (28.0, 2.0);
    let n = items.len() as f32;
    let seg = (w - 2.0 * pad) / n;
    let mut segs = row().key("segments");
    for (i, name) in items.iter().copied().enumerate() {
        let is = i == selected;
        segs = segs.child(
            el()
                .key(name)
                .role("segment")
                .label(name)
                .checked(is)
                .focusable(true)
                .size(seg, h - 2.0 * pad)
                .center()
                .radius(radius::SM)
                .hover(if is { Color::TRANSPARENT } else { p.t.hover })
                .on_press_tx(move |s: &mut App, tx| {
                    tx.instant();
                    on(s, i)
                })
                .child(
                    text(name)
                        .key("label")
                        .font(type_::LABEL)
                        .color(if is { p.t.text } else { p.t.text_secondary })
                        .a11y_hidden(),
                ),
        );
    }
    el()
        .key("segmented")
        .role("segmented")
        .size(w, h)
        .padding(pad)
        .radius(radius::MD)
        .fill(p.t.control_track)
        .child(
            el()
                .key("indicator")
                .size(seg, h - 2.0 * pad)
                .absolute(pad + selected as f32 * seg, pad)
                .radius(radius::SM)
                .fill(p.t.indicator)
                .shadow(1.0, 3.0, p.t.shadow)
                .shadow(0.0, 0.5, p.t.edge)
                .a11y_hidden(),
        )
        .child(segs)
}

fn swatch_row(p: Paint, key: &str, label: &str, current: [u8; 3], on: fn(&mut App, [u8; 3])) -> El {
    let (d, gap) = (22.0, 10.0);
    let ring = d + 8.0;
    let matched = SWATCHES.iter().position(|(_, rgb)| same(*rgb, current));
    let mut r = row().key(key).role("radiogroup").label(label).gap(gap).py(space::S2);
    if let Some(i) = matched {
        r = r.child(
            el()
                .key("ring")
                .size(ring, ring)
                .absolute(i as f32 * (d + gap) - 4.0, 4.0)
                .radius(ring / 2.0)
                .border(2.0, p.t.text)
                .a11y_hidden(),
        );
    }
    for (name, rgb) in SWATCHES {
        r = r.child(
            el()
                .key(name)
                .role("radio")
                .label(name)
                .checked(same(rgb, current))
                .focusable(true)
                .size(d, d)
                .radius(d / 2.0)
                .fill(swatch(rgb))
                .border(p.hair, Color::BLACK.alpha(0.12))
                .hover(p.t.hover)
                .on_press_tx(move |s: &mut App, tx| {
                    tx.instant();
                    on(s, rgb)
                }),
        );
    }

    row()
        .h(52.0)
        .px(space::S3)
        .justify_between()
        .center_y()
        .child(
            row()
                .gap(space::S2)
                .center_y()
                .child(
                    el()
                        .size(16.0, 16.0)
                        .radius(8.0)
                        .fill(swatch(current))
                        .border(p.hair, Color::BLACK.alpha(0.35))
                        .a11y_hidden(),
                )
                .child(text(label).font(type_::BODY).color(p.t.text))
                .child(
                    text(format!("#{:02x}{:02x}{:02x}", current[0], current[1], current[2]))
                        .font(type_::MONO_CAPTION)
                        .color(p.t.text_secondary),
                ),
        )
        .child(r)
}

fn toggle(p: Paint, on: bool) -> El {
    let (w, h, k) = (38.0, 22.0, 18.0);
    let inset = (h - k) / 2.0;
    el()
        .key("switch")
        .role("switch")
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on { p.accent } else { p.t.control_off })
        .child(
            el()
                .key("knob")
                .size(k, k)
                .absolute(if on { w - k - inset } else { inset }, inset)
                .radius(k / 2.0)
                .fill(p.t.knob)
                .shadow(1.0, 3.0, p.t.knob_shadow)
                .shadow(0.0, 0.5, p.t.edge)
                .a11y_hidden(),
        )
}

fn set_dpi_norm(s: &mut App, v: f32) {
    // The sensor moves in 100 DPI steps; off-grid values followed
    // inconsistently in practice.
    let dpi = (200.0 + v * 6200.0) / 100.0;
    s.dpi = (dpi.round() as u16 * 100).clamp(200, 6400);
}

fn setup(s: &mut App, waker: rune::Waker) {
    s.waker = Some(waker.clone());
    s.preview.waker = Some(waker);
}

fn poll(s: &mut App, _t: f64, _tx: &mut rune::Tx) -> bool {
    let msgs = {
        let mut q = s.results.lock().unwrap();
        std::mem::take(&mut *q)
    };
    let frame = s.preview.dirty.swap(false, Ordering::Relaxed);
    if msgs.is_empty() {
        return frame;
    }
    s.status = msgs.join(" · ");
    true
}

fn main() {
    let _ = rune::set_app_icon(include_bytes!("../assets/logo.png").to_vec());
    perch_core::window::polish_titlebar();
    rune::run(AppSpec::<App> {
        scenes: &[
            ("default", |_| {}),
            ("home", |s| s.tab = Tab::Home),
            ("mouse", |s| s.tab = Tab::Mouse),
            ("keyboard", |s| s.tab = Tab::Keyboard),
            ("camera", |s| s.tab = Tab::Camera),
        ],
        fonts: FONTS,
        families: ("Geist", "Geist Mono"),
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("Perch", WINDOW, init, view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No hardware: every commit path short-circuits with "not connected".
    fn detached() -> App {
        App {
            tab: Tab::Home,
            status: String::new(),
            mouse: None,
            dpi: 1600,
            polling: 1000,
            mouse_brightness: 100,
            mouse_effect: razer::Effect::Off,
            mouse_color: [255, 255, 255],
            keyboard: None,
            key_effect: redragon::Effect::ALL[0],
            key_color: [255, 0, 0],
            key_speed: 4,
            key_brightness: 4,
            camera: None,
            cam: Vec::new(),
            preview: Preview::new(),
            backdrop: backdrop(),
            waker: None,
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[test]
    fn sidebar_switches_and_the_slider_moves_state() {
        let mut rt = rune::Runtime::new(detached(), view, WINDOW, rune::Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("K616 Fizz Pro").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.tab, Tab::Keyboard);

        let (x, y) = rt.find("DeathAdder Essential").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.tab, Tab::Mouse);

        let (x, y) = rt.find("dpi-handle").unwrap();
        rt.pointer_down(x + 40.0, y);
        rt.pointer_move(x + 140.0, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.dpi > 1600, "dpi follows the drag: {}", rt.state.dpi);
        rt.pointer_up(x + 140.0, y);
    }

    #[test]
    fn the_dpi_slider_reaches_the_low_end() {
        let mut rt = rune::Runtime::new(detached(), view, WINDOW, rune::Env::new(true, 1.0));
        rt.frame(0.0);
        let (px, py) = rt.find("DeathAdder Essential").unwrap();
        rt.pointer_down(px, py);
        rt.pointer_up(px, py);
        rt.frame(1.0 / 60.0);
        let (x, y) = rt.find("dpi-handle").unwrap();
        rt.pointer_down(x - 118.0, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.dpi < 500, "dpi at the left edge: {}", rt.state.dpi);
        rt.pointer_up(x - 118.0, y);
    }

    #[test]
    fn a_swatch_sets_the_keyboard_colour() {
        let mut rt = rune::Runtime::new(detached(), view, WINDOW, rune::Env::new(true, 1.0));
        rt.frame(0.0);
        let (tx, ty) = rt.find("K616 Fizz Pro").unwrap();
        rt.pointer_down(tx, ty);
        rt.pointer_up(tx, ty);
        rt.frame(1.0 / 60.0);
        let (x, y) = rt.find("blue").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.key_color, [0, 0, 255]);
    }

    #[test]
    fn home_opens_the_device_it_names() {
        let mut rt = rune::Runtime::new(detached(), view, WINDOW, rune::Env::new(true, 1.0));
        rt.frame(0.0);
        assert_eq!(rt.state.tab, Tab::Home, "the app lands on Home");
        let (x, y) = rt.find("home-keyboard").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.tab, Tab::Keyboard);

        let (x, y) = rt.find("home-nav").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.tab, Tab::Home);
    }
}
