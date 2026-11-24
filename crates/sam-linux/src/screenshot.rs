//! One-shot screen capture through xdg-desktop-portal.
//!
//! GNOME Wayland does not let a normal client read the framebuffer. The
//! sanctioned door is `org.freedesktop.portal.Screenshot`, which asks the
//! portal (and, on a first use, the person) for one capture. This module only
//! asks for that capture; it never records, polls or keeps a screenshot.
//!
//! The reply is a temporary PNG that the caller owns. The caller must delete
//! it after reading the pixel it needs.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gio::prelude::*;

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Screenshot";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
const TIMEOUT_SECONDS: u32 = 45;

type Callback = Box<dyn FnOnce(Result<PathBuf, String>)>;

struct Capture {
    bus: gio::DBusConnection,
    callback: RefCell<Option<Callback>>,
    subscription: RefCell<Option<gio::SignalSubscription>>,
    timeout: RefCell<Option<glib::SourceId>>,
    done: Cell<bool>,
}

fn finish(capture: &Rc<Capture>, result: Result<PathBuf, String>) {
    if capture.done.replace(true) {
        return;
    }
    if let Some(timeout) = capture.timeout.borrow_mut().take() {
        timeout.remove();
    }
    // Dropping a `SignalSubscription` unsubscribes it.
    capture.subscription.borrow_mut().take();
    if let Some(callback) = capture.callback.borrow_mut().take() {
        callback(result);
    }
}

fn listen(capture: &Rc<Capture>, path: &str) {
    if capture.done.get() {
        return;
    }
    capture.subscription.borrow_mut().take();
    let subscription = capture.bus.subscribe_to_signal(
        Some(DEST),
        Some(REQUEST_IFACE),
        Some("Response"),
        Some(path),
        None,
        gio::DBusSignalFlags::NONE,
        {
            let capture = capture.clone();
            move |signal: gio::DBusSignalRef| {
                let params = signal.parameters;
                let Some((code, results)) =
                    params.get::<(u32, HashMap<String, glib::Variant>)>()
                else {
                    finish(&capture, Err("the screen portal sent an unreadable reply".into()));
                    return;
                };
                let uri: Option<String> =
                    results.get("uri").and_then(|value| value.get());
                match code {
                    0 => match uri {
                        Some(uri) => match gio::File::for_uri(&uri).path() {
                            Some(path) => finish(&capture, Ok(path)),
                            None => finish(
                                &capture,
                                Err("the screen portal returned no local file".into()),
                            ),
                        },
                        None => finish(
                            &capture,
                            Err("the screen portal returned no image".into()),
                        ),
                    },
                    1 => finish(&capture, Err("screen capture was cancelled".into())),
                    _ => finish(
                        &capture,
                        Err("the screen portal could not capture the screen".into()),
                    ),
                }
            }
        },
    );
    capture.subscription.replace(Some(subscription));
}

fn request_path(bus: &gio::DBusConnection, token: &str) -> String {
    let sender = bus
        .unique_name()
        .map(|name| name.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

fn token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("sam{}_{n}", std::process::id())
}

/// Ask the portal for one screenshot. `callback` runs on the caller's main
/// context, with a temporary PNG path or a reason it did not happen.
pub fn capture_async<F>(callback: F)
where
    F: FnOnce(Result<PathBuf, String>) + 'static,
{
    let bus = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(bus) => bus,
        Err(error) => {
            callback(Err(format!(
                "screen capture needs a session bus ({error})"
            )));
            return;
        }
    };
    let capture = Rc::new(Capture {
        bus: bus.clone(),
        callback: RefCell::new(Some(Box::new(callback))),
        subscription: RefCell::new(None),
        timeout: RefCell::new(None),
        done: Cell::new(false),
    });

    let token = token();
    let path = request_path(&bus, &token);
    listen(&capture, &path);

    let timeout = {
        let timer_capture = capture.clone();
        glib::timeout_add_seconds_local(TIMEOUT_SECONDS, move || {
            finish(
                &timer_capture,
                Err("screen capture timed out or was not allowed".into()),
            );
            glib::ControlFlow::Break
        })
    };
    capture.timeout.replace(Some(timeout));

    let options = glib::VariantDict::new(None);
    options.insert("handle_token", token);
    options.insert("interactive", false);
    options.insert("modal", true);
    let arguments = ("", options.end());

    {
        let capture = capture.clone();
        let expected = path.clone();
        bus.call(
            Some(DEST),
            PATH,
            IFACE,
            "Screenshot",
            Some(&arguments.to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            gio::Cancellable::NONE,
            move |result| match result {
                Ok(reply) => {
                    let Some(handle) = reply.child_value(0).get::<String>() else {
                        finish(
                            &capture,
                            Err("the screen portal returned no request handle".into()),
                        );
                        return;
                    };
                    if handle != expected {
                        // The spec derives the handle from the token, but if a
                        // portal chooses another path, follow the real one.
                        listen(&capture, &handle);
                    }
                }
                Err(error) => finish(
                    &capture,
                    Err(format!(
                        "screen capture is unavailable ({error}); install xdg-desktop-portal"
                    )),
                ),
            },
        );
    }
}
