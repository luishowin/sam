//! What `/help` shows: everything Sam can do, and how.
//! Static text, answered locally with no provider and no network.

/// The exact input that asks for help.
pub const QUESTION: &str = "/help";
/// Who answers it.
pub const HELP_BY: &str = "Sam";

pub fn text() -> &'static str {
    "Search: type an app, file, folder or setting name. Enter opens it; \
    Ctrl+Enter shows a file in Files.\n\
    Files: `find ...` searches names and contents through LocalSearch.\n\
    Paths: `~` or `/` completes folders and files.\n\
    Numbers: `17 * 42` or `100 km in miles`; Enter copies the result.\n\
    System: RAM, CPU, disk, battery and charging, network and IP, Bluetooth, GPU, display, brightness, audio and volume, time and date, temperature and uptime.\n\
    Quick actions: lock, suspend (asks first), log out, restart, power off, volume, media, screen brightness, Wi-Fi, Bluetooth, airplane mode, night light, dark style, focus mode, power profile, terminal, timers and alarms.\n\
    Speech: hold Tab or the microphone to speak a query. It is transcribed on this computer.\n\
    Web: an out-of-domain query offers `Search the web`, which opens your browser. Sam itself never browses.\n\
    `/help` shows this."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_names_every_door() {
        let help = text();
        for marker in [
            "Search:",
            "`find ...`",
            "`~`",
            "copies the result",
            "asks first",
            "night light",
            "brightness",
            "hold Tab or the microphone",
            "Search the web",
            "`/help`",
        ] {
            assert!(help.contains(marker), "help should mention {marker}");
        }
    }
}
