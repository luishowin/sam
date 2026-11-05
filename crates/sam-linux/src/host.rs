//! What the core may ask about this machine (§4): where the places are, and
//! what time it is here.

use std::path::PathBuf;

use sam_core::engine::{Host, PlaceInfo};
use sam_core::files::Clock;

use crate::config::tilde;

pub struct LinuxHost;

fn special(dir: glib::UserDirectory) -> Option<PathBuf> {
    glib::user_special_dir(dir)
}

impl Host for LinuxHost {
    fn place(&self, id: &str) -> Option<PlaceInfo> {
        use glib::UserDirectory as U;
        let (title, path, icon) = match id {
            "trash" => {
                return Some(PlaceInfo {
                    title: "Trash".into(),
                    subtitle: "trash:///".into(),
                    uri: "trash:///".into(),
                    icon: "user-trash-symbolic".into(),
                });
            }
            "recent" => {
                return Some(PlaceInfo {
                    title: "Recent".into(),
                    subtitle: "Recently used files".into(),
                    uri: "recent:///".into(),
                    icon: "document-open-recent-symbolic".into(),
                });
            }
            "home" => ("Home", Some(glib::home_dir()), "user-home-symbolic"),
            "desktop" => ("Desktop", special(U::Desktop), "user-desktop-symbolic"),
            "documents" => (
                "Documents",
                special(U::Documents),
                "folder-documents-symbolic",
            ),
            "downloads" => (
                "Downloads",
                special(U::Downloads),
                "folder-download-symbolic",
            ),
            "music" => ("Music", special(U::Music), "folder-music-symbolic"),
            "pictures" => ("Pictures", special(U::Pictures), "folder-pictures-symbolic"),
            "videos" => ("Videos", special(U::Videos), "folder-videos-symbolic"),
            "templates" => (
                "Templates",
                special(U::Templates),
                "folder-templates-symbolic",
            ),
            "public" => (
                "Public",
                special(U::PublicShare),
                "folder-publicshare-symbolic",
            ),
            // Where GNOME Shell saves screenshots.
            "screenshots" => (
                "Screenshots",
                special(U::Pictures).map(|p| p.join("Screenshots")),
                "camera-photo-symbolic",
            ),
            _ => return None,
        };
        let path = path.filter(|p| p.is_dir())?;
        let uri = glib::filename_to_uri(&path, None).ok()?.to_string();
        Some(PlaceInfo {
            title: title.into(),
            subtitle: tilde(&path),
            uri,
            icon: icon.into(),
        })
    }

    fn clock(&self) -> Clock {
        match glib::DateTime::now_local() {
            Ok(now) => Clock {
                now: now.to_unix(),
                utc_offset: now.utc_offset().as_seconds() as i32,
            },
            Err(_) => Clock {
                now: 0,
                utc_offset: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_resolve_on_this_machine() {
        let home = LinuxHost.place("home").expect("home exists");
        assert_eq!(home.subtitle, "~");
        assert!(home.uri.starts_with("file:///"));
        assert_eq!(LinuxHost.place("trash").unwrap().uri, "trash:///");
        assert!(LinuxHost.place("nowhere").is_none());
        for id in sam_core::action::PLACES {
            // Every place the core may ask about has an answer (or is absent here).
            let _ = LinuxHost.place(id);
        }
    }

    #[test]
    fn the_clock_is_now() {
        let c = LinuxHost.clock();
        assert!(c.now > 1_700_000_000);
        assert!(c.utc_offset.abs() <= 14 * 3600);
    }
}
