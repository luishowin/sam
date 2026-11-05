//! Sam's panel: a search pill, a results card, and honest execution.
//!
//! There is no model in this path. Text is matched against the registry and
//! live local data; Enter validates one inert intent and the executor carries
//! it out. The microphone is an input method only.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib};

use sam_core::files::FileQuery;
use sam_core::policy::{self, Approved, CORE_LEVEL, Decision, Origin, Pending};
use sam_core::rank::{Candidate, Kind};
use sam_core::trouble::Trouble;
use sam_linux::executor::Executor;
use sam_linux::log;
use sam_voice::{Event as VoiceEvent, VoiceEngine};

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

/// Actions that start something and may run before the panel hides.
const LAUNCHES: &[&str] = &["app.launch", "file.open", "file.reveal", "place.open", "url.open"];

pub struct Panel {
    window: adw::ApplicationWindow,
    entry: gtk::Entry,
    mic: gtk::Button,
    card: Surface,
    list: gtk::ListBox,
    footer: gtk::Label,
    world: World,
    executor: Executor,
    rows: RefCell<Vec<Candidate>>,
    generation: Cell<u64>,
    pending: RefCell<Option<(usize, Pending)>>,
    file_source: RefCell<Option<glib::SourceId>>,
    voice: Option<VoiceEngine>,
    listening: Cell<bool>,
    from_voice: Cell<bool>,
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
            .tooltip_text("Hold to speak (local, offline)")
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
            .label("↑↓ choose · ↵ open · esc close")
            .build();

        let card_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["sam-card-content"])
            .build();
        card_content.append(&list);
        card_content.append(&footer);
        let card = Surface::natural(&card_content, SURFACE_WIDTH, CARD_RADIUS);
        card.set_visible(false);

        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(GAP)
            .margin_top(TOP)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .build();
        frame.append(&pill);
        frame.append(&card);
        window.set_content(Some(&frame));

        let voice = VoiceEngine::spawn_default().ok();
        let panel = Rc::new(Panel {
            window: window.clone(),
            entry,
            mic,
            card,
            list,
            footer,
            world: World::new(),
            executor: Executor,
            rows: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            pending: RefCell::new(None),
            file_source: RefCell::new(None),
            voice,
            listening: Cell::new(false),
            from_voice: Cell::new(false),
        });

        let p = panel.clone();
        panel.entry.connect_changed(move |entry| {
            if p.listening.get() && !p.from_voice.get() {
                p.stop_listen();
            }
            p.refresh(entry.text().to_string());
        });

        let p = panel.clone();
        panel.list.connect_row_activated(move |_, row| {
            let index = row.index().max(0) as usize;
            p.activate(index);
        });

        // Push-to-talk: hold the mic while speaking, release to stop.
        let gesture = gtk::GestureClick::new();
        let p = panel.clone();
        gesture.connect_pressed(move |_, _, _, _| p.start_listen());
        let p = panel.clone();
        gesture.connect_released(move |_, _, _, _| p.stop_listen());
        panel.mic.add_controller(gesture);

        let keys = gtk::EventControllerKey::new();
        let p = panel.clone();
        keys.connect_key_pressed(move |_, key, _, _| p.key(key));
        panel.window.add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        let p = panel.clone();
        focus.connect_leave(move |_| {
            if !crate::trace::pinned() {
                p.hide("focus");
            }
        });
        panel.window.add_controller(focus);

        let p = panel.clone();
        panel.window.connect_close_request(move |_| {
            p.hide("close");
            glib::Propagation::Stop
        });

        // Speech events arrive on the worker thread; drain them on the main
        // loop so the panel never blocks on model load or inference.
        if panel.voice.is_some() {
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
        self.entry.set_text("");
        self.rows.borrow_mut().clear();
        self.pending.borrow_mut().take();
        self.clear_rows();
        self.card.set_visible(false);
    }

    // ---- searching ---------------------------------------------------------

    fn refresh(self: &Rc<Self>, text: String) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.pending.borrow_mut().take();
        self.footer.set_label("↑↓ choose · ↵ open · esc close");

        if text.trim().is_empty() {
            self.rows.borrow_mut().clear();
            self.clear_rows();
            self.card.set_visible(false);
            return;
        }

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
        if LAUNCHES.contains(&action.as_str()) {
            self.hide("launch");
        } else {
            self.hide("run");
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
        match self.executor.execute(approved, ctx.upcast_ref()) {
            Ok(_) => {}
            Err(e) => {
                self.footer.set_label(&format!("{}: {e}", candidate.title));
                log::warn("action-failed", Some(&action), e);
            }
        }
    }

    fn say(&self, line: &str) {
        self.footer.set_label(line);
    }

    // ---- keyboard ----------------------------------------------------------

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::Escape => {
                if self.listening.get() {
                    self.cancel_listen();
                } else {
                    self.hide("escape");
                }
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(row) = self.list.selected_row() {
                    self.activate(row.index().max(0) as usize);
                } else if let Some(first) = self.list.row_at_index(0) {
                    self.list.select_row(Some(&first));
                    self.activate(0);
                }
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
        let Some(voice) = &self.voice else {
            self.footer
                .set_label("Speech is unavailable — run `sam voice setup`");
            return;
        };
        match voice.start() {
            Ok(()) => {
                self.listening.set(true);
                self.mic.add_css_class("listening");
                self.footer.set_label("Listening… release to stop");
                trace!("listen started");
            }
            Err(e) => self.footer.set_label(&format!("Microphone: {e}")),
        }
    }

    fn stop_listen(&self) {
        if !self.listening.replace(false) {
            return;
        }
        if let Some(voice) = &self.voice
            && let Err(e) = voice.stop()
        {
            self.footer.set_label(&format!("Speech: {e}"));
        }
        self.mic.remove_css_class("listening");
        if self.footer.text().starts_with("Listening") {
            self.footer.set_label("↑↓ choose · ↵ open · esc close");
        }
    }

    fn cancel_listen(&self) {
        if !self.listening.replace(false) {
            return;
        }
        if let Some(voice) = &self.voice {
            let _ = voice.cancel();
        }
        self.mic.remove_css_class("listening");
        self.footer.set_label("↑↓ choose · ↵ open · esc close");
    }

    fn poll_voice(self: &Rc<Self>) {
        let Some(voice) = &self.voice else {
            return;
        };
        for event in voice.drain_events() {
            match event {
                VoiceEvent::ModelsLoading => {
                    self.mic.set_tooltip_text(Some("Loading the local speech model…"));
                }
                VoiceEvent::ModelsReady => {
                    self.mic
                        .set_tooltip_text(Some("Hold to speak (local, offline)"));
                }
                VoiceEvent::Listening => {
                    self.listening.set(true);
                    self.mic.add_css_class("listening");
                    self.footer.set_label("Listening… release to stop");
                }
                VoiceEvent::Partial(text) => {
                    if !text.is_empty() {
                        self.from_voice.set(true);
                        self.entry.set_text(&text);
                        self.entry.set_position(-1);
                        self.from_voice.set(false);
                    }
                }
                VoiceEvent::Final(text) => {
                    if !text.is_empty() {
                        self.from_voice.set(true);
                        self.entry.set_text(&text);
                        self.entry.set_position(-1);
                        self.from_voice.set(false);
                    }
                    self.listening.set(false);
                    self.mic.remove_css_class("listening");
                    self.footer.set_label("↑↓ choose · ↵ open · esc close");
                }
                VoiceEvent::Failed(text) => {
                    self.listening.set(false);
                    self.mic.remove_css_class("listening");
                    self.footer.set_label(&format!("Speech: {text}"));
                }
            }
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

/// The little bits the panel needs to keep alive.
pub fn keep(_: &Rc<Panel>) {}
