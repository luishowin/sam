//! Sam's panel: a search pill, a results card, and honest execution.
//!
//! There is no model in this path. Text is matched against the registry and
//! live local data; Enter validates one inert intent and the executor carries
//! it out. The microphone is an input method only.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib, graphene};

use sam_core::files::FileQuery;
use sam_core::policy::{self, Approved, CORE_LEVEL, Decision, Origin, Pending};
use sam_core::rank::{Candidate, Kind};
use sam_core::trouble::Trouble;
use sam_linux::config::Config;
use sam_linux::executor::Executor;
use sam_linux::log;
use sam_voice::{Event as VoiceEvent, VoiceConfig, VoiceEngine};

use crate::row::Row;
use crate::surface::{Shape, Surface};
use crate::trace::trace;
use crate::world::World;

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
    list: gtk::ListBox,
    footer: gtk::Label,
    world: World,
    executor: Executor,
    rows: RefCell<Vec<Candidate>>,
    generation: Cell<u64>,
    pending: RefCell<Option<(usize, Pending)>>,
    file_source: RefCell<Option<glib::SourceId>>,
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
            list,
            footer,
            world: World::new(),
            executor: Executor,
            rows: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            pending: RefCell::new(None),
            file_source: RefCell::new(None),
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
        self.rows.borrow_mut().clear();
        self.pending.borrow_mut().take();
        self.clear_rows();
        self.card.set_visible(false);
        self.timer_card.set_visible(false);
    }

    /// Whether a press at window coordinates lands on one of the visible
    /// surfaces. Everything else in the transparent window dismisses it.
    fn point_in_panel(&self, x: f64, y: f64) -> bool {
        let point = graphene::Point::new(x as f32, y as f32);
        [&self.pill, &self.card, &self.timer_card]
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

    // ---- searching ---------------------------------------------------------

    fn refresh(self: &Rc<Self>, text: String) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.pending.borrow_mut().take();
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

    fn render(self: &Rc<Self>, mut candidates: Vec<Candidate>) {
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.truncate(ROWS);
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
        let Some(candidate) = self.rows.borrow().get(index).cloned() else {
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

        let registry = self.world.engine.registry();
        let validated = match registry.validate(candidate.intent.clone()) {
            Ok(v) => v,
            Err(e) => {
                self.say(&format!("{} cannot run: {e}", candidate.intent.action));
                return;
            }
        };
        match policy::decide(CORE_LEVEL, validated, Origin::UserSelected) {
            Decision::Approved(approved) => self.run(approved, candidate),
            Decision::NeedsConfirmation(pending) => {
                self.pending.replace(Some((index, pending)));
                self.footer
                    .set_label("Press ↵ again to confirm · esc cancels");
                if let Some(row) = self.list.row_at_index(index as i32) {
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
        let keep_open = matches!(
            action.as_str(),
            "timer.set" | "timer.cancel" | "timer.list" | "alarm.list" | "help.show" | "info.copy"
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
