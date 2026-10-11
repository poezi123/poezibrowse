use std::cell::RefCell;
use std::rc::Rc;

use gtk::cairo;
use gtk::glib;
use gtk::prelude::*;

use crate::config::{Config, Mode};

const DRIFT_PIXELS_PER_SECOND: f64 = 8.0;
const TEXTURE: &[u8] = include_bytes!("../assets/titelleiste.png");

thread_local! {
    static PATTERN: Option<cairo::ImageSurface> =
        cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(TEXTURE)).ok();
}

#[derive(Default)]
struct State {
    pattern: bool,
    animation: bool,
    fps: u32,
    mode: Option<Mode>,
    offset: f64,
    last_us: i64,
    tick: Option<gtk::TickCallbackId>,
}

#[derive(Clone)]
pub struct TitleBar {
    header: gtk::HeaderBar,
    state: Rc<RefCell<State>>,
}

impl TitleBar {
    pub fn install(header: &gtk::HeaderBar, config: &Config, mode: Mode) -> TitleBar {
        header.style_context().add_class("poezi");
        let state = Rc::new(RefCell::new(State { mode: Some(mode), ..State::default() }));
        let s = state.clone();
        header.connect_draw(move |widget, cr| {
            draw(widget, cr, &s.borrow());
            glib::Propagation::Proceed
        });
        let bar = TitleBar { header: header.clone(), state };
        bar.apply(config);
        bar
    }

    pub fn apply(&self, config: &Config) {
        let animate = {
            let mut s = self.state.borrow_mut();
            s.pattern = config.titlebar_pattern;
            s.animation = config.titlebar_animation;
            s.fps = config.titlebar_fps.clamp(1, 60);
            s.pattern && s.animation
        };
        let running = self.state.borrow().tick.is_some();
        if animate && !running {
            self.state.borrow_mut().last_us = 0;
            let s = self.state.clone();
            let id = self.header.add_tick_callback(move |widget, clock| {
                tick(widget, clock, &mut s.borrow_mut());
                glib::ControlFlow::Continue
            });
            self.state.borrow_mut().tick = Some(id);
        } else if !animate && running {
            let mut s = self.state.borrow_mut();
            if let Some(id) = s.tick.take() {
                id.remove();
            }
            s.offset = 0.0;
        }
        self.header.queue_draw();
    }
}

fn period(widget: &gtk::HeaderBar) -> f64 {
    PATTERN.with(|p| {
        let Some(p) = p else { return 0.0 };
        let height = widget.allocated_height() as f64;
        if p.height() <= 0 || height <= 0.0 {
            return 0.0;
        }
        2.0 * p.width() as f64 * (height / p.height() as f64)
    })
}

fn tick(widget: &gtk::HeaderBar, clock: &gtk::gdk::FrameClock, s: &mut State) {
    let now = clock.frame_time();
    if s.last_us == 0 {
        s.last_us = now;
        return;
    }
    if now - s.last_us < 1_000_000 / s.fps.max(1) as i64 {
        return;
    }
    s.offset += (now - s.last_us) as f64 / 1e6 * DRIFT_PIXELS_PER_SECOND;
    s.last_us = now;
    let period = period(widget);
    if period > 0.0 && s.offset >= period {
        s.offset %= period;
    }
    widget.queue_draw();
}

fn draw(widget: &gtk::HeaderBar, cr: &cairo::Context, s: &State) {
    let width = widget.allocated_width() as f64;
    let height = widget.allocated_height() as f64;
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let ground = cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
    ground.add_color_stop_rgb(0.00, 0.008, 0.020, 0.055);
    ground.add_color_stop_rgb(0.35, 0.006, 0.010, 0.125);
    ground.add_color_stop_rgb(0.70, 0.045, 0.004, 0.105);
    ground.add_color_stop_rgb(1.00, 0.072, 0.004, 0.090);
    let _ = cr.set_source(&ground);
    cr.rectangle(0.0, 0.0, width, height);
    let _ = cr.fill();

    if s.pattern {
        PATTERN.with(|p| {
            let Some(p) = p else { return };
            let scale = height / p.height() as f64;
            let pattern = cairo::SurfacePattern::create(p);
            pattern.set_extend(cairo::Extend::Reflect);
            pattern.set_filter(cairo::Filter::Good);
            let mut matrix = cairo::Matrix::identity();
            matrix.scale(1.0 / scale, 1.0 / scale);
            matrix.translate(-s.offset, 0.0);
            pattern.set_matrix(matrix);
            let _ = cr.set_source(&pattern);
            cr.rectangle(0.0, 0.0, width, height);
            let _ = cr.fill();
        });
    }

    let tint = match s.mode {
        Some(Mode::Private) => Some([(0.42, 0.10, 0.62, 0.55), (0.62, 0.08, 0.42, 0.45)]),
        Some(Mode::Tor) => Some([(0.49, 0.27, 0.60, 0.60), (0.20, 0.55, 0.35, 0.40)]),
        _ => None,
    };
    if let Some([a, b]) = tint {
        let gradient = cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
        gradient.add_color_stop_rgba(0.0, a.0, a.1, a.2, a.3);
        gradient.add_color_stop_rgba(1.0, b.0, b.1, b.2, b.3);
        let _ = cr.set_source(&gradient);
        cr.rectangle(0.0, 0.0, width, height);
        let _ = cr.fill();
    }
}
