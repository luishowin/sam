//! The one public door to a side effect (§3, F1): it takes an
//! [`Approved`] intent and nothing else. Actions that reach the network
//! (§15) go through [`Executor::ask`] and [`Executor::weather`], and from
//! there through the network door (F5). Listening goes through
//! [`Executor::listen`], and from there through the microphone door
//! (F30).

use std::path::Path;

use sam_core::action::Handler;
use sam_core::entity::Entity;
use sam_core::policy::Approved;
use sam_core::result::{ExecutionResult, Status};
use sam_core::trouble::Trouble;

use crate::{ExecError, launch, system};

pub struct Executor;

fn arg<'a>(approved: &'a Approved, name: &str) -> Result<&'a str, ExecError> {
    approved
        .text(name)
        .ok_or_else(|| ExecError(format!("{} is missing {name}", approved.action())))
}

/// Entities an execution leaves behind for later reference ("open that").
/// Only actions with a stable identity today: launched apps and opened or
/// revealed files. Everything else genuinely has no id scheme yet (0.0.6-F).
/// Public so the panel records the same entities the executor returns.
pub fn entities_for(approved: &Approved) -> Vec<Entity> {
    let key = match approved.action() {
        "app.launch" => approved.text("app_id").map(|id| format!("app:{id}")),
        "file.open" | "file.reveal" => approved.text("uri").map(|uri| format!("uri:{uri}")),
        _ => None,
    };
    let Some(key) = key else {
        return Vec::new();
    };
    let name = key.rsplit(['/', ':']).next().unwrap_or(&key).to_owned();
    Entity::from_key(&key, &name).into_iter().collect()
}

impl Executor {
    /// Carries out `approved`. `ctx` is the panel's launch context, so what
    /// starts gets an activation token (§10). Returns a line for the user.
    /// A shim over [`execute_result`](Executor::execute_result): callers
    /// move to the structured result as the panel learns to render statuses.
    pub fn execute(
        &self,
        approved: Approved,
        ctx: &gio::AppLaunchContext,
    ) -> Result<String, ExecError> {
        let result = self.execute_result(approved, ctx);
        match result.status {
            Status::Success => Ok(result.output.unwrap_or_else(|| "Done".into())),
            _ => Err(ExecError(
                result.error.unwrap_or_else(|| "it did not work".into()),
            )),
        }
    }

    /// Carries out `approved` and says truthfully what happened (0.0.6-F).
    /// Verification is capability-specific: `file.open` and `file.reveal`
    /// check the target exists first (a deleted file fails here, not in
    /// Files); `place.open` resolves the folder first; `app.launch` fails
    /// when no desktop entry resolves. `Success` then means the OS handler
    /// accepted the action — launched, opened, switched, told. What a
    /// Wayland client cannot see (a mapped window, a focused app) is not
    /// claimed: post-effect confirmation needs compositor hooks, which is
    /// 0.1's business, not this release's. Launched apps and opened files
    /// come back as entities for later reference ("open that").
    pub fn execute_result(
        &self,
        approved: Approved,
        ctx: &gio::AppLaunchContext,
    ) -> ExecutionResult {
        match approved.spec().handler {
            Handler::Os => {}
            Handler::Panel => {
                return ExecutionResult::failed(format!(
                    "{} is carried out by the panel",
                    approved.action()
                ));
            }
            Handler::Net => {
                return ExecutionResult::failed(format!(
                    "{} reaches the network, through Executor::ask or ::weather",
                    approved.action()
                ));
            }
            Handler::Mic => {
                return ExecutionResult::failed(format!(
                    "{} opens the microphone, through Executor::listen",
                    approved.action()
                ));
            }
        }
        let a = &approved;
        let mut timer_entry: Option<crate::timers::Entry> = None;
        let outcome: Result<String, ExecError> = (|| -> Result<String, ExecError> {
            match a.action() {
            "app.launch" => launch::app(arg(a, "app_id")?, &[], ctx).map(|()| "Opened".into()),
            "file.open" => launch::uri(arg(a, "uri")?, ctx).map(|()| "Opened".into()),
            "file.reveal" => launch::reveal(arg(a, "uri")?, ctx).map(|()| "Shown in Files".into()),
            "place.open" => {
                let place = arg(a, "place")?;
                let info = sam_core::engine::Host::place(&crate::host::LinuxHost, place)
                    .ok_or_else(|| ExecError("that folder does not exist here".into()))?;
                launch::uri(&info.uri, ctx).map(|()| format!("Opened {}", info.title))
            }
            "url.open" => launch::uri(arg(a, "url")?, ctx).map(|()| "Opened".into()),
            "doc.open" => {
                let page = arg(a, "page")?;
                if page.is_empty()
                    || !page
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
                {
                    return Err(ExecError("that is not a valid manual page name".into()));
                }
                launch::uri(&format!("man:{page}"), ctx).map(|()| "Opened the manual".into())
            }
            "calendar.open" => launch::app("org.gnome.Calendar.desktop", &[], ctx)
                .map(|()| "Opened Calendar".into()),
            "system.lock" => system::lock(),
            "system.suspend" => system::suspend(),
            "session.logout" => system::logout(),
            "session.restart" => system::restart(),
            "session.power_off" => system::power_off(),
            "bluetooth.set" => system::bluetooth(arg(a, "state")?),
            "wifi.set" => system::wifi(arg(a, "state")?),
            "audio.volume" => system::volume_step(arg(a, "direction")?),
            "audio.volume_set" => system::volume_set(
                a.number("level")
                    .ok_or_else(|| ExecError("audio.volume_set is missing level".into()))?,
            ),
            "audio.mute" => system::mute(arg(a, "state")?),
            "media.control" => system::media(arg(a, "command")?),
            "appearance.dark" => system::dark(arg(a, "state")?),
            "notifications.dnd" => system::dnd(arg(a, "state")?),
            "display.night_light" => system::night_light(arg(a, "state")?),
            "display.night_light_temperature" => system::night_light_temperature(
                a.number("temperature")
                    .ok_or_else(|| ExecError("night light needs a temperature".into()))?,
            ),
            "system.airplane" => system::airplane(arg(a, "state")?),
            "power.profile" => system::power_profile(arg(a, "profile")?),
            "media.seek" => system::media_seek(
                arg(a, "direction")?,
                a.number("seconds")
                    .ok_or_else(|| ExecError("seek needs seconds".into()))?,
            ),
            "bluetooth.connect" => {
                system::bluetooth_connect(arg(a, "action")?, arg(a, "device")?)
            }
            "terminal.open" => launch::terminal(ctx),
            "timer.set" => {
                let started =
                    crate::timers::set_entry(arg(a, "duration")?, a.text("label").unwrap_or(""))?;
                timer_entry = Some(started.entry);
                Ok(started.message)
            }
            "timer.cancel" => crate::timers::cancel(a.text("id").unwrap_or("")),
            "timer.list" => crate::timers::list(),
            "alarm.set" => crate::timers::alarm(arg(a, "time")?, a.text("label").unwrap_or("")),
            "alarm.cancel" => crate::timers::cancel(a.text("id").unwrap_or("")),
            "alarm.list" => crate::timers::list(),
                other => Err(ExecError(format!(
                    "the executor does not know {other}"
                ))),
            }
        })();
        match outcome {
            Ok(line) => {
                let mut result = ExecutionResult::success().with_output(line);
                if let Some(entry) = timer_entry {
                    result = result
                        .with_meta("timer_id", entry.id)
                        .with_meta("timer_due", entry.due.to_string())
                        .with_meta("timer_label", entry.label);
                }
                for entity in entities_for(&approved) {
                    result = result.with_entity(entity);
                }
                result
            }
            Err(error) => ExecutionResult::failed(error.0),
        }
    }

}
