//! `SAM_TRACE=1`: timings on stderr (the journal, under systemd).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use gtk::prelude::*;
use gtk::{gdk, glib};

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SAM_TRACE").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// `SAM_PIN=1`: losing focus does not hide the panel (for screenshots and
/// the GTK inspector; Esc and Super+Space still do).
pub fn pinned() -> bool {
    static PINNED: OnceLock<bool> = OnceLock::new();
    *PINNED.get_or_init(|| std::env::var_os("SAM_PIN").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// Milliseconds between two `glib::monotonic_time()` readings.
pub fn ms(from_us: i64, to_us: i64) -> f64 {
    (to_us - from_us) as f64 / 1000.0
}

macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::trace::on() {
            eprintln!("sam[{}] {}", glib::monotonic_time() / 1000, format_args!($($arg)*));
        }
    };
}
pub(crate) use trace;

/// Frame intervals and per-frame work on a widget's frame clock, from
/// `start` until `finish` (§11's animation budget).
pub struct FrameStats {
    clock: gdk::FrameClock,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    paint_started: Cell<i64>,
    last_painted: Cell<Option<i64>>,
    /// Painted frame to painted frame, in ms.
    intervals: RefCell<Vec<f64>>,
    /// Before-paint to after-paint (update, layout, paint), in ms.
    work: RefCell<Vec<f64>>,
}

impl FrameStats {
    pub fn start(widget: &impl IsA<gtk::Widget>) -> Option<Rc<FrameStats>> {
        let clock = widget.frame_clock()?;
        let stats = Rc::new(FrameStats {
            clock: clock.clone(),
            handlers: RefCell::default(),
            paint_started: Cell::new(0),
            last_painted: Cell::new(None),
            intervals: RefCell::default(),
            work: RefCell::default(),
        });
        let weak = Rc::downgrade(&stats);
        let before = clock.connect_before_paint(move |_| {
            if let Some(stats) = weak.upgrade() {
                stats.paint_started.set(glib::monotonic_time());
            }
        });
        let weak = Rc::downgrade(&stats);
        let after = clock.connect_after_paint(move |_| {
            let Some(stats) = weak.upgrade() else { return };
            let now = glib::monotonic_time();
            stats
                .work
                .borrow_mut()
                .push(ms(stats.paint_started.get(), now));
            if let Some(last) = stats.last_painted.replace(Some(now)) {
                stats.intervals.borrow_mut().push(ms(last, now));
            }
        });
        stats.handlers.replace(vec![before, after]);
        Some(stats)
    }

    /// Stops listening and summarises. Dropping it unfinished only stops
    /// listening.
    pub fn finish(&self) -> String {
        for handler in self.handlers.take() {
            self.clock.disconnect(handler);
        }
        let summary = |values: &mut Vec<f64>| {
            if values.is_empty() {
                return "none".to_owned();
            }
            values.sort_by(f64::total_cmp);
            let at = |q: f64| values[((values.len() - 1) as f64 * q).round() as usize];
            format!(
                "p50 {:.1} · p95 {:.1} · max {:.1} ms",
                at(0.5),
                at(0.95),
                at(1.0)
            )
        };
        let mut intervals = self.intervals.take();
        let mut work = self.work.take();
        let slow = intervals.iter().filter(|v| **v > 20.0).count();
        format!(
            "{} frames; interval {}; work {}; {slow} intervals over 20 ms",
            work.len(),
            summary(&mut intervals),
            summary(&mut work)
        )
    }
}

impl Drop for FrameStats {
    fn drop(&mut self) {
        for handler in self.handlers.take() {
            self.clock.disconnect(handler);
        }
    }
}
