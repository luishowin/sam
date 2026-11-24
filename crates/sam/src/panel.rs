//! Sam's panel: a search pill, a results card, and honest execution.
//!
//! There is no model in the fast path. Text is matched against the registry and
//! live local data; Enter validates one inert intent and the executor carries
//! it out. A model may later supply a candidate call, but it travels the same
//! validation and policy path. The microphone is an input method only.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::gdk::prelude::TextureExtManual;
use gtk::{gdk, glib, graphene};

use sam_core::files::FileQuery;
use sam_core::intent::{Intent, Source};
use sam_core::policy::{self, Approved, CORE_LEVEL, Decision, Origin, Pending};
use sam_core::rank::{Candidate, Kind};
use sam_linux::color::Rgb;
use sam_core::trouble::Trouble;
use sam_linux::config::Config;
use sam_linux::executor::Executor;
use sam_linux::log;
use sam_voice::{Event as VoiceEvent, VoiceConfig, VoiceEngine};

use crate::row::Row;
use crate::surface::{Shape, Surface};
use crate::trace::trace;
use crate::world::{NeedleReply, World};

const FRAME_WIDTH: i32 = 760;
const FRAME_HEIGHT: i32 = 500;
const SURFACE_WIDTH: f64 = 728.0;
const PILL_HEIGHT: f64 = 58.0;
const CARD_RADIUS: f64 = 20.0;
const TOP: i32 = 10;
const GAP: i32 = 8;
const ROWS: usize = 8;
const ROW_HEIGHT: i32 = 44;
const FILE_DELAY: Duration = Duration::from_millis(80);
/// A single word waits this long before the dictionary is asked. Typing
/// normally never touches it.
const DEFINITION_DELAY: Duration = Duration::from_millis(250);
const MIC_MARK: i32 = 16;

const PLACEHOLDER: &str = "Search files, apps, settings or system";
const FOOTER_IDLE: &str = "↑↓ choose · ↵ open · hold tab to speak · esc close";

/// Actions that start something and may run before the panel hides.
const LAUNCHES: &[&str] = &["app.launch", "file.open", "file.reveal", "place.open", "url.open"];

pub struct Panel {
    window: adw::ApplicationWindow,
    entry: gtk::Entry,
    mic: gtk::Button,
    pill: Surface,
    card: Surface,
    timer_card: Surface,
    timer_clock: gtk::Label,
    timer_sub: gtk::Label,
    timer_button: gtk::Button,
    timer_due: Cell<Option<u64>>,
    timer_id: RefCell<Option<String>>,
    timer_source: RefCell<Option<glib::SourceId>>,
    detail_card: Surface,
    detail_title: gtk::Label,
    detail_body: gtk::Label,
    detail_swatch: gtk::DrawingArea,
    detail_copy: gtk::Button,
    detail_text: RefCell<String>,
    detail_color: Rc<RefCell<Option<gdk::RGBA>>>,
    color_window: RefCell<Option<gtk::Window>>,
    list: gtk::ListBox,
    footer: gtk::Label,
    world: World,
    executor: Executor,
    rows: RefCell<Vec<Candidate>>,
    generation: Cell<u64>,
    pending: RefCell<Option<(usize, Pending)>>,
    file_source: RefCell<Option<glib::SourceId>>,
    definition_source: RefCell<Option<glib::SourceId>>,
    definition: RefCell<Option<Candidate>>,
    definition_cache: RefCell<HashMap<String, Option<Candidate>>>,
    voice: RefCell<Option<VoiceEngine>>,
    voice_model: PathBuf,
    voice_mode: String,
    voice_threads: usize,
    voice_ready: Cell<bool>,
    pending_start: Cell<bool>,
    tab_held: Cell<bool>,
    listening: Cell<bool>,
    from_voice: Cell<bool>,
    voice_cancelled: Cell<bool>,
    voice_auto_run: bool,
}

impl Panel {
    pub fn new(app: &adw::Application) -> Rc<Panel> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .default_width(FRAME_WIDTH)
            .default_height(FRAME_HEIGHT)
            .build();
        window.set_css_classes(&["sam-window"]);

        let entry = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text(PLACEHOLDER)
            .css_classes(["sam-entry"])
            .build();

        let mic = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("audio-input-microphone-symbolic"))
            .valign(gtk::Align::Center)
            .tooltip_text("Hold Tab or the microphone to speak (local, offline)")
            .css_classes(["sam-mic"])
            .build();
        if let Some(image) = mic.child().and_then(|c| c.downcast::<gtk::Image>().ok()) {
            image.set_pixel_size(MIC_MARK);
        }

        let search = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["sam-search"])
            .build();
        search.append(&entry);
        search.append(&mic);

        let pill = Surface::new(&search, Shape::pill(SURFACE_WIDTH, PILL_HEIGHT));

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .css_classes(["sam-results"])
            .build();

        let footer = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["sam-footer"])
            .label(FOOTER_IDLE)
            .build();

        let card_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["sam-card-content"])
            .build();
        card_content.append(&list);
        card_content.append(&footer);
        let card = Surface::natural(&card_content, SURFACE_WIDTH, CARD_RADIUS);
        card.set_visible(false);

        let timer_clock = gtk::Label::builder()
            .label("00:00")
            .xalign(0.5)
            .css_classes(["sam-timer-clock"])
            .build();
        let timer_sub = gtk::Label::builder()
            .label("Sam timer")
            .xalign(0.5)
            .css_classes(["sam-timer-sub"])
            .build();
        let timer_button = gtk::Button::builder()
            .label("Cancel timer")
            .halign(gtk::Align::Center)
            .css_classes(["sam-timer-button"])
            .build();
        let timer_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(22)
            .margin_bottom(18)
            .margin_start(20)
            .margin_end(20)
            .build();
        timer_content.append(&timer_clock);
        timer_content.append(&timer_sub);
        timer_content.append(&timer_button);
        let timer_card = Surface::natural(&timer_content, SURFACE_WIDTH, CARD_RADIUS);
        timer_card.set_visible(false);

        let detail_title = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["sam-detail-title"])
            .build();
        let detail_body = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .css_classes(["sam-detail-body"])
            .build();
        let detail_color: Rc<RefCell<Option<gdk::RGBA>>> = Rc::new(RefCell::new(None));
        let detail_swatch = gtk::DrawingArea::builder()
            .width_request(44)
            .height_request(44)
            .halign(gtk::Align::Start)
            .visible(false)
            .css_classes(["sam-detail-swatch"])
            .build();
        {
            let color = detail_color.clone();
            detail_swatch.set_draw_func(move |_, cr, _, _| {
                if let Some(color) = *color.borrow() {
                    cr.set_source_rgba(
                        f64::from(color.red()),
                        f64::from(color.green()),
                        f64::from(color.blue()),
                        f64::from(color.alpha()),
                    );
                    let _ = cr.paint();
                }
            });
        }
        let detail_copy = gtk::Button::builder()
            .label("Copy")
            .halign(gtk::Align::Start)
            .css_classes(["sam-detail-copy"])
            .build();
        // Long definitions scroll instead of growing the panel past the
        // screen; the title, swatch and Copy stay put.
        let detail_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(280)
            .hexpand(true)
            .child(&detail_body)
            .build();
        let detail_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(20)
            .margin_bottom(16)
            .margin_start(20)
            .margin_end(20)
            .build();
        detail_content.append(&detail_title);
        detail_content.append(&detail_swatch);
        detail_content.append(&detail_scroll);
        detail_content.append(&detail_copy);
        let detail_card = Surface::natural(&detail_content, SURFACE_WIDTH, CARD_RADIUS);
        detail_card.follow_content(FRAME_HEIGHT - TOP - PILL_HEIGHT as i32 - GAP - 16);
        detail_card.set_visible(false);

        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(GAP)
            .margin_top(TOP)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .build();
        frame.append(&pill);
        frame.append(&card);
        frame.append(&timer_card);
        frame.append(&detail_card);
        window.set_content(Some(&frame));

        let config = Config::load();
        let voice_mode = config.voice_mode().to_owned();
        let voice_model = config.voice.model_path();
        let voice = if voice_mode == "eager" {
            spawn_voice(config.voice.threads, Some(voice_model.clone()))
        } else {
            None
        };
        let panel = Rc::new(Panel {
            window: window.clone(),
            entry,
            mic,
            pill,
            card,
            timer_card,
            timer_clock,
            timer_sub,
            timer_button,
            timer_due: Cell::new(None),
            timer_id: RefCell::new(None),
            timer_source: RefCell::new(None),
            detail_card,
            detail_title,
            detail_body,
            detail_swatch,
            detail_copy,
            detail_text: RefCell::new(String::new()),
            detail_color,
            color_window: RefCell::new(None),
            list,
            footer,
            world: World::new(),
            executor: Executor,
            rows: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            pending: RefCell::new(None),
            file_source: RefCell::new(None),
            definition_source: RefCell::new(None),
            definition: RefCell::new(None),
            definition_cache: RefCell::new(HashMap::new()),
            voice: RefCell::new(voice),
            voice_model,
            voice_mode,
            voice_threads: config.voice.threads,
            voice_ready: Cell::new(false),
            pending_start: Cell::new(false),
            tab_held: Cell::new(false),
            listening: Cell::new(false),
            from_voice: Cell::new(false),
            voice_cancelled: Cell::new(false),
            voice_auto_run: config.voice.auto_run,
        });

        if panel.voice_mode == "off" {
            panel.mic.set_sensitive(false);
            panel
                .mic
                .set_tooltip_text(Some("Speech is off — run `sam config`"));
        }

        let p = panel.clone();
        panel.timer_button.connect_clicked(move |_| {
            p.timer_button_clicked();
        });

        let p = panel.clone();
        panel.detail_copy.connect_clicked(move |button| {
            let text = p.detail_text.borrow().clone();
            if text.is_empty() {
                return;
            }
            gtk::prelude::WidgetExt::display(&p.window)
                .clipboard()
                .set_text(&text);
            button.set_label("Copied");
        });

        let p = panel.clone();
        panel.entry.connect_activate(move |_| {
            p.activate_selected();
        });

        let p = panel.clone();
        panel.entry.connect_changed(move |entry| {
            if p.listening.get() && !p.from_voice.get() {
                p.cancel_listen();
            }
            p.refresh(entry.text().to_string());
        });

        let p = panel.clone();
        panel.list.connect_row_activated(move |_, row| {
            let index = row.index().max(0) as usize;
            p.activate(index);
        });

        // Push-to-talk: hold Tab or the mic while speaking, release to stop.
        let gesture = gtk::GestureClick::new();
        let p = panel.clone();
        gesture.connect_pressed(move |_, _, _, _| p.start_listen());
        let p = panel.clone();
        gesture.connect_released(move |_, _, _, _| p.stop_listen());
        panel.mic.add_controller(gesture);

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let p = panel.clone();
        keys.connect_key_pressed(move |_, key, _, _| p.key(key));
        let p = panel.clone();
        keys.connect_key_released(move |_, key, _, _| p.key_released(key));
        panel.window.add_controller(keys);

        // The window is a fixed-size transparent overlay, so a press below
        // the visible surfaces would otherwise be swallowed. Dismiss on any
        // press outside the pill, results card and timer card.
        let outside = gtk::GestureClick::new();
        outside.set_propagation_phase(gtk::PropagationPhase::Capture);
        let p = panel.clone();
        outside.connect_pressed(move |gesture, _, x, y| {
            if p.point_in_panel(x, y) {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            p.hide("outside");
        });
        panel.window.add_controller(outside);

        let focus = gtk::EventControllerFocus::new();
        let p = panel.clone();
        focus.connect_leave(move |_| {
            if crate::trace::pinned() {
                return;
            }
            if p.timer_card.is_visible() && p.timer_due.get().is_some() {
                return;
            }
            if p.detail_card.is_visible() {
                return;
            }
            p.hide("focus");
        });
        panel.window.add_controller(focus);

        let p = panel.clone();
        panel.window.connect_close_request(move |_| {
            p.hide("close");
            glib::Propagation::Stop
        });

        // Speech events arrive on the worker thread; drain them on the main
        // loop so the panel never blocks on model load or inference.
        if panel.voice_mode != "off" {
            let p = panel.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || {
                p.poll_voice();
                glib::ControlFlow::Continue
            });
        }

        panel
    }

    // ---- showing and hiding -------------------------------------------------

    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide("toggle");
        } else {
            self.show();
        }
    }

    pub fn show(self: &Rc<Self>) {
        self.world.projects.refresh_if_stale();
        self.window.present();
        self.entry.grab_focus();
        if self.timer_due.get().is_some_and(|due| due > now_secs()) {
            self.card.set_visible(false);
            self.timer_card.set_visible(true);
        }
        trace!("shown");
    }

    pub fn show_text(self: &Rc<Self>, text: &str) {
        self.show();
        self.entry.set_text(text);
        self.entry.set_position(-1);
    }

    pub fn hide(&self, why: &str) {
        if self.window.is_visible() {
            trace!("hidden: {why}");
        }
        self.window.set_visible(false);
        self.stop_listen();
        self.pending_start.set(false);
        self.tab_held.set(false);
        self.entry.set_text("");
        if let Some(source) = self.definition_source.borrow_mut().take() {
            source.remove();
        }
        self.definition.replace(None);
        self.rows.borrow_mut().clear();
        self.pending.borrow_mut().take();
        self.clear_rows();
        self.card.set_visible(false);
        self.timer_card.set_visible(false);
        self.detail_card.set_visible(false);
        self.detail_text.replace(String::new());
        self.detail_color.replace(None);
    }

    /// Whether a press at window coordinates lands on one of the visible
    /// surfaces. Everything else in the transparent window dismisses it.
    fn point_in_panel(&self, x: f64, y: f64) -> bool {
        let point = graphene::Point::new(x as f32, y as f32);
        [&self.pill, &self.card, &self.timer_card, &self.detail_card]
            .into_iter()
            .filter(|surface| surface.is_visible())
            .any(|surface| {
                let widget: &gtk::Widget = surface.upcast_ref();
                widget
                    .compute_bounds(&self.window)
                    .is_some_and(|bounds| bounds.contains_point(&point))
            })
    }

    // ---- timer card --------------------------------------------------------

    fn timer_button_clicked(self: &Rc<Self>) {
        if self.timer_due.get().is_some() {
            self.cancel_timer();
        } else {
            self.hide("timer-close");
        }
    }

    fn show_timer(self: &Rc<Self>, id: String, due: u64, label: &str) {
        self.timer_due.set(Some(due));
        self.timer_id.replace(Some(id));
        self.timer_button.set_label("Cancel timer");
        let when = glib::DateTime::from_unix_local(due as i64)
            .ok()
            .and_then(|d| d.format("%H:%M").ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        self.timer_sub
            .set_label(&format!("{label} · ends at {when}"));
        self.card.set_visible(false);
        self.timer_card.set_visible(true);
        self.update_timer();
        if self.timer_due.get().is_none() {
            return;
        }
        if let Some(source) = self.timer_source.borrow_mut().take() {
            source.remove();
        }
        let panel = self.clone();
        let source = glib::timeout_add_seconds_local(1, move || {
            panel.timer_source.borrow_mut().take();
            if panel.update_timer() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *self.timer_source.borrow_mut() = Some(source);
    }

    fn update_timer(&self) -> bool {
        let Some(due) = self.timer_due.get() else {
            return false;
        };
        let now = now_secs();
        if due <= now {
            self.timer_due.set(None);
            self.timer_id.replace(None);
            self.timer_clock.set_label("00:00");
            self.timer_sub.set_label("Time's up");
            self.timer_button.set_label("Close");
            return false;
        }
        let left = due - now;
        self.timer_clock.set_label(&format_remaining(left));
        true
    }

    fn cancel_timer(self: &Rc<Self>) {
        if let Some(id) = self.timer_id.borrow().clone() {
            let _ = sam_linux::timers::cancel(&id);
        }
        self.timer_due.set(None);
        self.timer_id.replace(None);
        if let Some(source) = self.timer_source.borrow_mut().take() {
            source.remove();
        }
        self.timer_card.set_visible(false);
        self.hide("timer-cancelled");
    }

    // ---- detail card: definitions and colours ------------------------------

    fn show_detail(self: &Rc<Self>, title: &str, body: &str, copy: &str, color: Option<Rgb>) {
        self.detail_body.set_use_markup(false);
        self.detail_body.set_label(body);
        self.present_detail(title, copy, color);
    }

    fn show_detail_markup(
        self: &Rc<Self>,
        title: &str,
        markup: &str,
        copy: &str,
        color: Option<Rgb>,
    ) {
        self.detail_body.set_use_markup(true);
        self.detail_body.set_markup(markup);
        self.present_detail(title, copy, color);
    }

    fn present_detail(&self, title: &str, copy: &str, color: Option<Rgb>) {
        self.detail_title.set_label(title);
        self.detail_text.replace(copy.to_owned());
        self.detail_color.replace(color.map(|c| {
            gdk::RGBA::new(
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
                1.0,
            )
        }));
        self.detail_swatch.set_visible(color.is_some());
        self.detail_swatch.queue_draw();
        self.detail_copy.set_label("Copy");
        self.card.set_visible(false);
        self.timer_card.set_visible(false);
        self.detail_card.set_visible(true);
        self.window.present();
    }

    fn show_dictionary(self: &Rc<Self>, word: &str) {
        let entry = match sam_linux::dictionary::entry(word) {
            Ok(entry) => entry,
            Err(error) => {
                self.say(&error);
                return;
            }
        };
        let title = if entry.hyphenated.is_empty() {
            entry.word.clone()
        } else {
            entry.hyphenated.to_owned()
        };
        let plain = sam_linux::dictionary::render_plain(&entry);
        let markup = dictionary_markup(&entry);
        self.show_detail_markup(&title, &markup, &plain, None);
    }

    /// Ask the portal for one screenshot, then show it for a click.
    fn start_color_pick(self: &Rc<Self>) {
        self.card.set_visible(false);
        self.detail_card.set_visible(false);
        self.footer.set_label("Waiting for the screen capture…");
        self.window.present();
        let panel = self.clone();
        sam_linux::screenshot::capture_async(move |result| match result {
            Ok(path) => panel.pick_from_image(path),
            Err(error) => {
                panel.window.present();
                panel.say(&error);
            }
        });
    }

    fn pick_from_image(self: &Rc<Self>, path: PathBuf) {
        let texture = match gdk::Texture::from_filename(&path) {
            Ok(texture) => texture,
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                self.say(&format!("the screenshot could not be read ({error})"));
                return;
            }
        };
        // The portal hands us a temporary file. Read it once, then remove it;
        // the pixels live only in memory until the picker closes.
        let _ = std::fs::remove_file(&path);

        let window = gtk::Window::builder()
            .title("Pick a colour")
            .transient_for(&self.window)
            .modal(true)
            .default_width(960)
            .default_height(640)
            .build();
        window.set_css_classes(&["sam-picker-window"]);

        let hint = gtk::Label::builder()
            .label("Click the colour you want · Esc cancels")
            .css_classes(["sam-picker-hint"])
            .build();
        let picture = gtk::Picture::builder()
            .paintable(&texture)
            .content_fit(gtk::ContentFit::Contain)
            .hexpand(true)
            .vexpand(true)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();
        content.append(&hint);
        content.append(&picture);
        window.set_child(Some(&content));

        // The controllers are owned by the window and its picture, so their
        // closures must hold weak references: a strong one would make a
        // window → controller → window cycle and leak the whole screenshot.
        let click = gtk::GestureClick::new();
        let panel = Rc::downgrade(self);
        let picker = window.downgrade();
        let view = picture.downgrade();
        let image = texture.clone();
        click.connect_pressed(move |_, _, x, y| {
            let (Some(view), Some(picker)) = (view.upgrade(), picker.upgrade()) else {
                return;
            };
            if let Some(color) = sample(&image, &view, x, y) {
                picker.close();
                if let Some(panel) = panel.upgrade() {
                    panel.show_color(color);
                }
            }
        });
        picture.add_controller(click);

        let keys = gtk::EventControllerKey::new();
        let picker = window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(picker) = picker.upgrade() {
                    picker.close();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);

        {
            let panel = Rc::downgrade(self);
            window.connect_close_request(move |_| {
                if let Some(panel) = panel.upgrade() {
                    panel.color_window.replace(None);
                    // A click closes the window just before showing the
                    // colour, so only an unpicked close resets the footer.
                    if !panel.detail_card.is_visible() {
                        panel.footer.set_label("Colour pick cancelled");
                    }
                }
                glib::Propagation::Proceed
            });
        }

        self.color_window.replace(Some(window.clone()));
        window.present();
    }

    fn show_color(self: &Rc<Self>, color: Rgb) {
        let body = color.describe();
        self.show_detail("Colour picked", &body, &body, Some(color));
        self.footer.set_label("Copy keeps the hex, RGB and HSL values");
    }

    // ---- searching ---------------------------------------------------------

    fn refresh(self: &Rc<Self>, text: String) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.pending.borrow_mut().take();
        if let Some(source) = self.definition_source.borrow_mut().take() {
            source.remove();
        }
        self.definition.replace(None);
        self.footer.set_label(FOOTER_IDLE);

        if text.trim().is_empty() {
            self.rows.borrow_mut().clear();
            self.clear_rows();
            self.card.set_visible(false);
            if self.window.is_visible() && self.timer_due.get().is_some() {
                self.timer_card.set_visible(true);
            }
            return;
        }
        self.timer_card.set_visible(false);
        self.detail_card.set_visible(false);

        let results = self.world.search(&text);
        let mut candidates = self.world.panel_rows(&results, ROWS * 2);
        let query = results.query.files.clone();

        let strong = candidates
            .first()
            .is_some_and(|c| c.score >= 0.9 && matches!(c.kind, Kind::Info | Kind::Command | Kind::Calculation | Kind::Conversion | Kind::Site));
        if strong {
            candidates.retain(|c| c.kind != Kind::Search);
        }
        self.render(candidates);

        // Files arrive after a short pause, unless a strong deterministic
        // answer or command already owns the top of the list.
        if let Some(source) = self.file_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(query) = query.filter(|_| !strong) {
            let panel = self.clone();
            let source = glib::timeout_add_local_once(FILE_DELAY, move || {
                panel.file_source.borrow_mut().take();
                panel.search_files(query, generation);
            });
            *self.file_source.borrow_mut() = Some(source);
        }
        self.schedule_definition(&text, generation);
    }

    /// Ask the dictionary only after a pause on one ordinary word.
    fn schedule_definition(self: &Rc<Self>, text: &str, generation: u64) {
        let Some(word) = single_word(text) else {
            return;
        };
        let word = word.to_owned();
        let panel = self.clone();
        let source = glib::timeout_add_local_once(DEFINITION_DELAY, move || {
            panel.definition_source.borrow_mut().take();
            if panel.generation.get() != generation {
                return;
            }
            if !single_word(&panel.entry.text()).is_some_and(|now| now.eq_ignore_ascii_case(&word))
            {
                return;
            }
            let Some(row) = panel.definition_row(&word) else {
                return;
            };
            panel.definition.replace(Some(row));
            let rows: Vec<Candidate> = panel
                .rows
                .borrow()
                .iter()
                .filter(|candidate| candidate.kind != Kind::Definition)
                .cloned()
                .collect();
            panel.render(rows);
        });
        *self.definition_source.borrow_mut() = Some(source);
    }

    /// Build (or reuse) the preview row. The cache keeps backspacing and
    /// retyping from doing the same binary search again.
    fn definition_row(&self, word: &str) -> Option<Candidate> {
        let key = word.to_lowercase();
        if let Some(cached) = self.definition_cache.borrow().get(&key) {
            return cached.clone();
        }
        let row = sam_linux::dictionary::preview(&key).map(|preview| Candidate {
            title: word.to_owned(),
            subtitle: format!("{} · {}", preview.pos, preview.definition),
            kind: Kind::Definition,
            icon: "accessories-dictionary-symbolic".into(),
            score: 0.0,
            key: format!("definition:{}", preview.word),
            intent: Intent::new("dictionary.lookup", Source::Provider).arg("word", word),
            alt: None,
        });
        let mut cache = self.definition_cache.borrow_mut();
        if cache.len() >= 96 {
            cache.clear();
        }
        cache.insert(key, row.clone());
        row
    }

    fn search_files(self: &Rc<Self>, query: FileQuery, generation: u64) {
        if self.generation.get() != generation {
            return;
        }
        match self.world.files.search(&query) {
            Ok(rows) => {
                if self.generation.get() != generation {
                    return;
                }
                let mut candidates = self.rows.borrow().clone();
                candidates.extend(rows);
                self.render(candidates);
            }
            Err(e) => {
                log::warn(Trouble::FilesUnavailable.code(), Some("search.files"), e);
            }
        }
    }

    fn render(self: &Rc<Self>, candidates: Vec<Candidate>) {
        let candidates = arrange(candidates, self.definition.borrow().clone());
        self.rows.replace(candidates.clone());
        self.clear_rows();

        for candidate in &candidates {
            let row = Row::new(candidate);
            self.list.append(&row.widget);
        }
        if let Some(first) = self.list.row_at_index(0) {
            self.list.select_row(Some(&first));
        }
        self.card.follow_content(FRAME_HEIGHT - TOP - PILL_HEIGHT as i32 - GAP - 16);
        self.card.set_visible(!candidates.is_empty());
    }

    fn clear_rows(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
    }

    // ---- execution ---------------------------------------------------------

    fn activate(self: &Rc<Self>, index: usize) {
        let Some(mut candidate) = self.rows.borrow().get(index).cloned() else {
            return;
        };

        if let Some((pending_index, _)) = self.pending.borrow().as_ref()
            && *pending_index == index
        {
            if let Some((_, pending)) = self.pending.borrow_mut().take() {
                self.run(pending.confirm(), candidate);
            }
            return;
        }

        // The deterministic engine only submits to the model where it has
        // no confident row of its own: a file-search fallback, a web
        // handoff, or nothing at all. Typing itself never waits for a model.
        let model_origin = candidate.kind != Kind::Definition
            && (candidate.key.starts_with("model:")
                || candidate.score < sam_core::engine::WEAK
                || matches!(candidate.kind, Kind::Search | Kind::Site));
        if model_origin && !candidate.key.starts_with("model:") {
            let text = self.entry.text().to_string();
            match self.world.needle_reply(&text) {
                NeedleReply::Candidate(model) => candidate = model,
                NeedleReply::Clarify(slot) => {
                    self.say(&format!("Which {slot}?"));
                    return;
                }
                NeedleReply::No => {}
            }
        }
        let origin = if candidate.key.starts_with("model:") {
            Origin::Model
        } else {
            Origin::UserSelected
        };

        let registry = self.world.engine.registry();
        let validated = match registry.validate(candidate.intent.clone()) {
            Ok(v) => v,
            Err(e) => {
                self.say(&format!("{} cannot run: {e}", candidate.intent.action));
                return;
            }
        };
        match policy::decide(CORE_LEVEL, validated, origin) {
            Decision::Approved(approved) => self.run(approved, candidate),
            Decision::NeedsConfirmation(pending) => {
                // A model result was not on screen when Enter was pressed.
                // Show it as the only row, then ask for the same yes the
                // panel already knows how to take.
                let pending_index = if candidate.key.starts_with("model:") {
                    self.render(vec![candidate]);
                    0
                } else {
                    index
                };
                self.pending.replace(Some((pending_index, pending)));
                self.footer
                    .set_label("Press ↵ again to confirm · esc cancels");
                if let Some(row) = self.list.row_at_index(pending_index as i32) {
                    row.add_css_class("sam-confirm");
                }
            }
            Decision::Denied(denial) => {
                self.say(&format!("{} is not allowed here", denial.action));
            }
        }
    }

    fn run(self: &Rc<Self>, approved: Approved, candidate: Candidate) {
        let action = approved.action().to_owned();
        let keep_open = action.starts_with("probe.")
            || matches!(
                action.as_str(),
                "timer.set"
                    | "timer.cancel"
                    | "timer.list"
                    | "alarm.list"
                    | "dictionary.lookup"
                    | "color.pick"
                    | "help.show"
                    | "info.copy"
                    | "window.list"
            );
        if !keep_open {
            if LAUNCHES.contains(&action.as_str()) {
                self.hide("launch");
            } else {
                self.hide("run");
            }
        }
        // Panel-owned results never reach the OS executor.
        match action.as_str() {
            "dictionary.lookup" => {
                if let Some(word) = approved.text("word") {
                    self.show_dictionary(word);
                }
                return;
            }
            "color.pick" => {
                self.start_color_pick();
                return;
            }
            "help.show" => {
                let text = sam_core::help::text().to_owned();
                gtk::prelude::WidgetExt::display(&self.window)
                    .clipboard()
                    .set_text(&text);
                self.footer.set_label("Help copied to the clipboard");
                return;
            }
            "info.copy" => {
                let text = approved.text("text").unwrap_or_default().to_owned();
                gtk::prelude::WidgetExt::display(&self.window)
                    .clipboard()
                    .set_text(&text);
                self.hide("copied");
                return;
            }
            "timer.list" | "alarm.list" => {
                if let Ok(list) = sam_linux::timers::list() {
                    self.footer.set_label(&list.replace('\n', " · "));
                }
                return;
            }
            "search.files" => {
                if let Some(query) = approved.text("query") {
                    self.entry.set_text(&format!("find {query}"));
                    self.entry.set_position(-1);
                }
                return;
            }
            _ => {}
        }
        let ctx = gtk::prelude::WidgetExt::display(&self.window).app_launch_context();
        let result = self.executor.execute_result(approved, ctx.upcast_ref());
        if !result.succeeded() {
            let why = result
                .error
                .clone()
                .unwrap_or_else(|| "it did not work".into());
            self.footer.set_label(&format!("{}: {why}", candidate.title));
            log::warn("action-failed", Some(&action), why);
            return;
        }
        match action.as_str() {
            "timer.set" => {
                let id = result.metadata.get("timer_id").cloned().unwrap_or_default();
                let due = result
                    .metadata
                    .get("timer_due")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                let label = result
                    .metadata
                    .get("timer_label")
                    .cloned()
                    .unwrap_or_else(|| "Sam timer".into());
                if due > now_secs() {
                    self.show_timer(id, due, &label);
                } else {
                    self.footer.set_label("The timer could not be started");
                }
            }
            "timer.cancel" => {
                self.timer_due.set(None);
                self.timer_id.replace(None);
                if let Some(source) = self.timer_source.borrow_mut().take() {
                    source.remove();
                }
                self.timer_card.set_visible(false);
                self.hide("timer-cancelled");
            }
            probe if probe.starts_with("probe.") => {
                if let Some(output) = result.output {
                    self.footer.set_label(&output.replace('\n', " · "));
                }
            }
            "window.list" => {
                if let Some(output) = result.output {
                    self.footer.set_label(&output.replace('\n', " · "));
                }
            }
            _ => {}
        }
    }

    fn say(&self, line: &str) {
        self.footer.set_label(line);
    }

    // ---- keyboard ----------------------------------------------------------

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::Tab => {
                if self.voice_mode == "off" {
                    return glib::Propagation::Proceed;
                }
                // Auto-repeat sends more presses while the key is held; only
                // the first one starts the utterance.
                if !self.tab_held.replace(true) {
                    self.start_listen();
                }
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if self.listening.get() {
                    self.cancel_listen();
                } else {
                    self.hide("escape");
                }
                glib::Propagation::Stop
            }
            gdk::Key::KP_Enter => {
                self.activate_selected();
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                self.move_selection(1);
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                self.move_selection(-1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    }

    fn key_released(&self, key: gdk::Key) {
        if key != gdk::Key::Tab || !self.tab_held.replace(false) {
            return;
        }
        // Release before the model is ready cancels the pending utterance.
        self.pending_start.set(false);
        self.stop_listen();
    }

    fn activate_selected(self: &Rc<Self>) {
        if let Some(row) = self.list.selected_row() {
            self.activate(row.index().max(0) as usize);
        } else if let Some(first) = self.list.row_at_index(0) {
            self.list.select_row(Some(&first));
            self.activate(0);
        }
    }

    /// Development only: what Enter does, callable over D-Bus.
    pub fn debug_enter(self: &Rc<Self>) {
        self.activate_selected();
    }

    /// Development only: the real push-to-talk path.
    pub fn debug_start_listen(self: &Rc<Self>) {
        self.start_listen();
    }

    pub fn debug_stop_listen(self: &Rc<Self>) {
        self.stop_listen();
    }

    /// Development only: what a final voice transcript does.
    pub fn debug_heard(self: &Rc<Self>, text: &str) {
        self.from_voice.set(true);
        self.entry.set_text(text);
        self.entry.set_position(-1);
        self.from_voice.set(false);
        self.run_voice_text(text);
    }

    fn move_selection(&self, delta: i32) {
        let count = self.rows.borrow().len() as i32;
        if count == 0 {
            return;
        }
        let current = self
            .list
            .selected_row()
            .map_or(0, |row| row.index())
            .max(0);
        let next = (current + delta).rem_euclid(count);
        if let Some(row) = self.list.row_at_index(next) {
            self.list.select_row(Some(&row));
        }
    }

    // ---- speech ------------------------------------------------------------

    fn start_listen(self: &Rc<Self>) {
        if self.voice_mode == "off" {
            self.footer.set_label("Speech is off — run `sam config`");
            return;
        }
        if self.voice.borrow().is_none() {
            self.pending_start.set(true);
            match spawn_voice(self.voice_threads, Some(self.voice_model.clone())) {
                Some(engine) => {
                    *self.voice.borrow_mut() = Some(engine);
                    self.footer
                        .set_label("Loading the speech model — keep holding");
                }
                None => {
                    self.pending_start.set(false);
                    self.footer.set_label("Speech: the worker could not start");
                }
            }
            return;
        }
        if !self.voice_ready.get() {
            self.pending_start.set(true);
            self.footer
                .set_label("Loading the speech model — keep holding");
            return;
        }
        self.pending_start.set(false);
        let voice = self.voice.borrow();
        let Some(voice) = voice.as_ref() else {
            return;
        };
        match voice.start() {
            Ok(()) => {
                self.voice_cancelled.set(false);
                self.listening.set(true);
                self.mic.add_css_class("listening");
                self.footer.set_label("Listening… release to stop");
                trace!("listen started");
            }
            Err(e) => self.footer.set_label(&format!("Microphone: {e}")),
        }
    }

    fn stop_listen(&self) {
        self.pending_start.set(false);
        if !self.listening.replace(false) {
            return;
        }
        if let Some(voice) = self.voice.borrow().as_ref()
            && let Err(e) = voice.stop()
        {
            self.footer.set_label(&format!("Speech: {e}"));
        }
        self.mic.remove_css_class("listening");
        if self.footer.text().starts_with("Listening") {
            self.footer.set_label(FOOTER_IDLE);
        }
    }

    fn cancel_listen(&self) {
        self.voice_cancelled.set(true);
        self.pending_start.set(false);
        if !self.listening.replace(false) {
            return;
        }
        if let Some(voice) = self.voice.borrow().as_ref() {
            let _ = voice.cancel();
        }
        self.mic.remove_css_class("listening");
        self.footer.set_label(FOOTER_IDLE);
    }

    fn poll_voice(self: &Rc<Self>) {
        let events: Vec<VoiceEvent> = match self.voice.borrow().as_ref() {
            Some(voice) => voice.drain_events(),
            None => return,
        };
        for event in events {
            match event {
                VoiceEvent::ModelsLoading => {
                    self.voice_ready.set(false);
                    self.mic.set_tooltip_text(Some("Loading the local speech model…"));
                }
                VoiceEvent::ModelsReady => {
                    self.voice_ready.set(true);
                    self.mic.set_tooltip_text(Some(
                        "Hold Tab or the microphone to speak (local, offline)",
                    ));
                    if self.pending_start.replace(false) {
                        self.start_listen();
                    }
                }
                VoiceEvent::Listening => {
                    self.listening.set(true);
                    self.mic.add_css_class("listening");
                    self.footer.set_label("Listening… release to stop");
                }
                VoiceEvent::Partial(text) => {
                    if !self.voice_cancelled.get() && !text.is_empty() {
                        self.from_voice.set(true);
                        self.entry.set_text(&text);
                        self.entry.set_position(-1);
                        self.from_voice.set(false);
                    }
                }
                VoiceEvent::Final(text) => {
                    let cancelled = self.voice_cancelled.replace(false);
                    self.listening.set(false);
                    self.mic.remove_css_class("listening");
                    if cancelled {
                        self.footer.set_label(FOOTER_IDLE);
                        continue;
                    }
                    if !text.is_empty() {
                        self.from_voice.set(true);
                        self.entry.set_text(&text);
                        self.entry.set_position(-1);
                        self.from_voice.set(false);
                        self.footer
                            .set_label(&format!("Heard “{text}” · running…"));
                        if self.voice_auto_run {
                            self.run_voice_text(&text);
                        }
                    } else {
                        self.footer.set_label("Nothing heard · try again");
                    }
                }
                VoiceEvent::Failed(text) => {
                    self.voice_ready.set(false);
                    self.pending_start.set(false);
                    self.listening.set(false);
                    self.mic.remove_css_class("listening");
                    self.footer.set_label(&format!("Speech: {text}"));
                }
            }
        }
    }

    /// Voice is an input method, not a separate reasoning path. After the
    /// final transcript, run one safe result: an explicit open verb, or a
    /// quick-action command. Information, files and search results stay on
    /// screen for review. A risky action still asks.
    fn run_voice_text(self: &Rc<Self>, text: &str) {
        let Some(top) = self.rows.borrow().first().cloned() else {
            self.footer.set_label("No match · refine or press ↵");
            return;
        };
        if top.score < 0.6 {
            self.footer.set_label("No confident match · review the list");
            return;
        }
        let words: Vec<String> = text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect();
        let first = words.first().map(String::as_str).unwrap_or("");
        if matches!(first, "find" | "search" | "locate" | "where") {
            self.footer
                .set_label("Heard · file results appear below, ↵ opens one");
            return;
        }
        let explicit_open = matches!(
            first,
            "open" | "launch" | "start" | "show" | "run" | "reveal"
        ) || text.to_lowercase().starts_with("go to ");
        let runnable = matches!(top.kind, Kind::Command)
            || (explicit_open
                && matches!(
                    top.kind,
                    Kind::App | Kind::Place | Kind::File | Kind::Folder | Kind::Setting
                        | Kind::Site
                ));
        if runnable {
            self.activate(0);
        } else {
            self.footer
                .set_label("Heard · review the result, ↵ runs it");
        }
    }

    // ---- development -------------------------------------------------------

    pub fn save_snapshot(&self, path: &str) {
        let width = self.window.width().max(1) as f32;
        let height = self.window.height().max(1) as f32;
        let paintable = gtk::WidgetPaintable::new(Some(&self.window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, width as f64, height as f64);
        let Some(node) = snapshot.to_node() else {
            return;
        };
        let Some(renderer) = self.window.renderer() else {
            return;
        };
        let viewport = gtk::graphene::Rect::new(0.0, 0.0, width, height);
        let texture = renderer.render_texture(&node, Some(&viewport));
        let _ = texture.save_to_png(path);
    }
}

/// Pango markup for a dictionary entry, in the spirit of the iOS card.
fn dictionary_markup(entry: &sam_linux::dictionary::Entry) -> String {
    use std::fmt::Write as _;

    let escape = |text: &str| glib::markup_escape_text(text).to_string();
    let mut out = String::new();
    if !entry.pronunciation.is_empty() {
        let _ = write!(
            out,
            "<span size=\"large\" style=\"italic\">{}</span>\n\n",
            escape(entry.pronunciation)
        );
    }
    let mut pos = "";
    let mut number = 0;
    for sense in &entry.senses {
        if sense.pos != pos {
            if !pos.is_empty() {
                out.push('\n');
            }
            let _ = write!(
                out,
                "<span weight=\"bold\" size=\"medium\">{}</span>\n",
                escape(sense.pos)
            );
            pos = sense.pos;
            number = 0;
        }
        number += 1;
        let _ = write!(
            out,
            "<b>{number}</b> {}\n",
            escape(sense.definition)
        );
        if !sense.example.is_empty() {
            let _ = write!(
                out,
                "<span style=\"italic\" alpha=\"72%\">e.g. {}</span>\n",
                escape(sense.example)
            );
        }
        if !sense.synonyms.is_empty() {
            let _ = write!(
                out,
                "<span size=\"small\"><b>SYNONYMS</b> {}</span>\n",
                escape(sense.synonyms)
            );
        }
        if !sense.antonyms.is_empty() {
            let _ = write!(
                out,
                "<span size=\"small\"><b>ANTONYMS</b> {}</span>\n",
                escape(sense.antonyms)
            );
        }
    }
    if !entry.origin.is_empty() {
        out.push('\n');
        let _ = write!(
            out,
            "<span size=\"small\" weight=\"bold\">ORIGIN</span>\n<span alpha=\"72%\">{}</span>\n",
            escape(entry.origin)
        );
    }
    out.trim_end().to_owned()
}

/// Order the visible rows. A definition is reserved a slot and pinned after
/// real results (and commands), but above a lone generic file-search row.
fn arrange(mut candidates: Vec<Candidate>, definition: Option<Candidate>) -> Vec<Candidate> {
    candidates.retain(|candidate| candidate.kind != Kind::Definition);
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let room = if definition.is_some() { ROWS - 1 } else { ROWS };
    candidates.truncate(room);
    if let Some(definition) = definition {
        let has_results = candidates.iter().any(|candidate| candidate.kind != Kind::Search);
        let at = if has_results {
            candidates
                .iter()
                .position(|candidate| candidate.kind == Kind::Search)
                .map_or(candidates.len(), |search| search + 1)
        } else {
            0
        };
        candidates.insert(at.min(candidates.len()), definition);
    }
    candidates
}

/// The one ordinary word worth asking the dictionary about.
fn single_word(text: &str) -> Option<&str> {
    let word = text.trim();
    if word.chars().count() < 2 || word.chars().any(char::is_whitespace) {
        return None;
    }
    word.chars()
        .all(|c| c.is_alphabetic() || c == '-' || c == '\'' || c == '’')
        .then_some(word)
}

/// Read one pixel from a `ContentFit::Contain` picture at a click.
///
/// The gesture coordinates are relative to the picture widget, so the same
/// aspect-fit arithmetic GTK uses maps them back to texture pixels.
fn sample(texture: &gdk::Texture, picture: &gtk::Picture, x: f64, y: f64) -> Option<Rgb> {
    let width = texture.width();
    let height = texture.height();
    if width <= 0 || height <= 0 {
        return None;
    }
    let view_width = f64::from(picture.width());
    let view_height = f64::from(picture.height());
    if view_width <= 0.0 || view_height <= 0.0 {
        return None;
    }
    let scale = (view_width / f64::from(width)).min(view_height / f64::from(height));
    let drawn_width = f64::from(width) * scale;
    let drawn_height = f64::from(height) * scale;
    let left = (view_width - drawn_width) / 2.0;
    let top = (view_height - drawn_height) / 2.0;
    let column = ((x - left) / scale).floor() as i32;
    let row = ((y - top) / scale).floor() as i32;
    if column < 0 || row < 0 || column >= width || row >= height {
        return None;
    }

    let stride = width as usize * 4;
    let mut pixels = vec![0u8; stride * height as usize];
    texture.download(&mut pixels, stride);
    let index = row as usize * stride + column as usize * 4;
    // gdk_texture_download returns CAIRO_FORMAT_ARGB32 in native byte order:
    // little-endian B, G, R, A. Screenshots are opaque, but un-premultiply
    // defensively so a transparent pixel does not come back too dark.
    let (blue, green, red, alpha) = (
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    );
    if alpha == 0 {
        return Some(Rgb::new(0, 0, 0));
    }
    let un = |channel: u8| {
        if alpha == 255 {
            channel
        } else {
            ((u16::from(channel) * 255 + u16::from(alpha) / 2) / u16::from(alpha)).min(255) as u8
        }
    };
    Some(Rgb::new(un(red), un(green), un(blue)))
}

fn spawn_voice(threads: usize, model: Option<PathBuf>) -> Option<VoiceEngine> {
    let mut config = VoiceConfig::default();
    config.model = model;
    if threads > 0 {
        config.threads = threads;
    }
    VoiceEngine::spawn(config).ok()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn format_remaining(seconds: u64) -> String {
    let h = seconds / 3_600;
    let m = (seconds % 3_600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// The little bits the panel needs to keep alive.
pub fn keep(_: &Rc<Panel>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, kind: Kind, score: f32) -> Candidate {
        Candidate {
            title: title.to_owned(),
            subtitle: String::new(),
            kind,
            icon: String::new(),
            score,
            key: title.to_owned(),
            intent: Intent::new("help.show", Source::Provider),
            alt: None,
        }
    }

    fn titles(rows: &[Candidate]) -> Vec<&str> {
        rows.iter().map(|row| row.title.as_str()).collect()
    }

    #[test]
    fn only_plain_single_words_trigger_a_definition() {
        assert_eq!(single_word("serendipity"), Some("serendipity"));
        assert_eq!(single_word("well-being"), Some("well-being"));
        assert_eq!(single_word("o'clock"), Some("o'clock"));
        assert_eq!(single_word("two words"), None);
        assert_eq!(single_word("find serendipity"), None);
        assert_eq!(single_word("a"), None);
        assert_eq!(single_word("C++"), None);
        assert_eq!(single_word("~/notes"), None);
        assert_eq!(single_word("42"), None);
    }

    #[test]
    fn definition_sits_at_the_bottom_when_results_exist() {
        let rows = arrange(
            vec![
                row("search", Kind::Search, 0.05),
                row("file", Kind::File, 0.9),
            ],
            Some(row("meaning", Kind::Definition, 0.0)),
        );
        assert_eq!(titles(&rows), ["file", "search", "meaning"]);
    }

    #[test]
    fn definition_leads_when_only_the_search_fallback_exists() {
        let rows = arrange(
            vec![row("search", Kind::Search, 0.05)],
            Some(row("meaning", Kind::Definition, 0.0)),
        );
        assert_eq!(titles(&rows), ["meaning", "search"]);
    }

    #[test]
    fn definition_stays_below_a_strong_command() {
        let rows = arrange(
            vec![row("command", Kind::Command, 0.97)],
            Some(row("meaning", Kind::Definition, 0.0)),
        );
        assert_eq!(titles(&rows), ["command", "meaning"]);
    }

    #[test]
    fn dictionary_markup_is_valid_pango() {
        let entry = sam_linux::dictionary::Entry {
            word: "colour".into(),
            pronunciation: "/ˈkʌl.ə(ɹ)/",
            hyphenated: "col·our".into(),
            origin: "From Middle English colour, from Anglo-Norman colur.",
            senses: vec![sam_linux::dictionary::Sense {
                pos: "noun",
                definition: "The spectral composition of visible light.",
                example: "Humans and birds can perceive colour.",
                synonyms: "hue, shade",
                antonyms: "achromia",
            }],
        };
        let markup = dictionary_markup(&entry);
        gtk::pango::parse_markup(&markup, '\0')
            .expect("the dictionary card is valid markup");
    }

    #[test]
    fn a_definition_never_displaces_a_result_into_a_ninth_row() {
        let mut rows: Vec<Candidate> = (0..8)
            .map(|i| row(&format!("file {i}"), Kind::File, 0.9 - i as f32 * 0.01))
            .collect();
        rows.push(row("duplicate", Kind::Definition, 0.0));
        let rows = arrange(rows, Some(row("meaning", Kind::Definition, 0.0)));
        assert_eq!(rows.len(), ROWS);
        assert_eq!(rows.last().unwrap().title, "meaning");
    }
}
