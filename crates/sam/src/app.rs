//! The GApplication: one instance per session, held alive with its window
//! hidden (§8). `toggle` is what Super+Space sends.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use sam_core::APP_ID;

use crate::panel::Panel;
use crate::trace::trace;

pub fn run() -> std::process::ExitCode {
    // The software renderer, unless told otherwise: for a panel this size it
    // paints as fast as Vulkan and keeps LLVM and the GPU drivers out of the
    // process — 36 MB less RSS (SPIKE-A).
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: nothing has started a thread yet; GLib's own threads start
        // when the application registers, below.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::ALLOW_REPLACEMENT)
        .build();

    app.connect_startup(|app| {
        let started = glib::monotonic_time();
        // The panel is onyx in any desktop style, so its text is light.
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        load_css();
        let panel = Panel::new(app);
        add_actions(app, &panel);
        // The window is hidden between summons and GApplication does not
        // count hidden windows, so hold explicitly for the life of the process.
        std::mem::forget(app.hold());
        trace!(
            "startup in {:.1} ms",
            crate::trace::ms(started, glib::monotonic_time())
        );
    });

    // `sam` from a shell or the app grid: show, never hide.
    app.connect_activate(|app| {
        app.activate_action("show", None);
    });

    // GApplication parses its own options (--gapplication-service, …).
    app.run().into()
}

fn add_actions(app: &adw::Application, panel: &Rc<Panel>) {
    let toggle = {
        let panel = panel.clone();
        gio::ActionEntry::builder("toggle")
            .activate(move |_: &adw::Application, _, _| panel.toggle())
            .build()
    };
    let show = {
        let panel = panel.clone();
        gio::ActionEntry::builder("show")
            .activate(move |_: &adw::Application, _, _| panel.show())
            .build()
    };
    // `sam show TEXT`: the panel with TEXT typed. It fills the entry and
    // nothing more; Enter is still the user's.
    let show_text = {
        let panel = panel.clone();
        gio::ActionEntry::builder("show-text")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(move |_: &adw::Application, _, param| {
                let text = param.and_then(|p| p.get::<String>()).unwrap_or_default();
                panel.show_text(&text);
            })
            .build()
    };
    let hide = {
        let panel = panel.clone();
        gio::ActionEntry::builder("hide")
            .activate(move |_: &adw::Application, _, _| panel.hide("action"))
            .build()
    };
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    app.add_action_entries([toggle, show, show_text, hide, quit]);
    // Development: `gapplication action io.github.luishowin.Sam debug-snapshot "'/tmp/x.png'"`.
    #[cfg(debug_assertions)]
    {
        let snapshot = {
            let panel = panel.clone();
            gio::ActionEntry::builder("debug-snapshot")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(move |_: &adw::Application, _, param| {
                    if let Some(path) = param.and_then(|p| p.get::<String>()) {
                        panel.save_snapshot(&path);
                    }
                })
                .build()
        };
        let enter = {
            let panel = panel.clone();
            gio::ActionEntry::builder("debug-enter")
                .activate(move |_: &adw::Application, _, _| panel.debug_enter())
                .build()
        };
        let heard = {
            let panel = panel.clone();
            gio::ActionEntry::builder("debug-heard")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(move |_: &adw::Application, _, param| {
                    if let Some(text) = param.and_then(|p| p.get::<String>()) {
                        panel.debug_heard(&text);
                    }
                })
                .build()
        };
        let listen_start = {
            let panel = panel.clone();
            gio::ActionEntry::builder("debug-listen-start")
                .activate(move |_: &adw::Application, _, _| panel.debug_start_listen())
                .build()
        };
        let listen_stop = {
            let panel = panel.clone();
            gio::ActionEntry::builder("debug-listen-stop")
                .activate(move |_: &adw::Application, _, _| panel.debug_stop_listen())
                .build()
        };
        app.add_action_entries([snapshot, enter, heard, listen_start, listen_stop]);
    }
}

/// `sam show [TEXT]`: asks the running SAM (starting it if needed) to show
/// the panel with TEXT typed.
pub fn show_remote(text: &str) -> std::process::ExitCode {
    let result = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).and_then(|bus| {
        let path = format!("/{}", APP_ID.replace('.', "/"));
        let platform_data = std::collections::HashMap::<String, glib::Variant>::new();
        let args = ("show-text", vec![text.to_variant()], platform_data).to_variant();
        bus.call_sync(
            Some(APP_ID),
            &path,
            "org.freedesktop.Application",
            "ActivateAction",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            gio::Cancellable::NONE,
        )
    });
    match result {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sam: could not reach SAM: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
