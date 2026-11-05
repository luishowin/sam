//! First-class entities (§0.0.6-A). IDA stops treating every result as an
//! unstructured string: a search hit becomes an `Entity` with an identity,
//! so a later request ("open that", "close that app") resolves against the
//! entity rather than rediscovering it from raw text.
//!
//! The id scheme reuses the engine's dedupe keys: `app:<desktop id>` and
//! `uri:<uri>` (places use their URI too). Nothing here touches the OS.

use std::collections::BTreeMap;
use std::fmt;

/// An entity id: `app:org.gnome.Calculator.desktop`,
/// `file:///home/luis/Downloads/x.pdf`, `weather:nairobi`, …
pub type EntityId = String;

/// What kind of thing an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntityType {
    App,
    File,
    Folder,
    Window,
    Weather,
    /// Anything else a capability returns. Prefer a precise type.
    Other,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EntityType::App => "app",
            EntityType::File => "file",
            EntityType::Folder => "folder",
            EntityType::Window => "window",
            EntityType::Weather => "weather",
            EntityType::Other => "other",
        };
        f.write_str(s)
    }
}

/// A thing IDA can refer back to.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub entity_type: EntityType,
    pub display_name: String,
    pub metadata: BTreeMap<String, String>,
}

impl Entity {
    pub fn new(
        id: impl Into<String>,
        entity_type: EntityType,
        display_name: impl Into<String>,
    ) -> Self {
        Entity {
            id: id.into(),
            entity_type,
            display_name: display_name.into(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.metadata.insert(key.to_owned(), value.into());
        self
    }

    /// `app:firefox` style reference used in prompts and traces.
    /// The id after the prefix is never logged with arguments (F6); the
    /// reference names the identity, not the content.
    pub fn reference(&self) -> String {
        format!("{}:{}", self.entity_type, self.short_id())
    }

    fn short_id(&self) -> &str {
        match self.id.split_once(':') {
            Some((_, rest)) if !rest.is_empty() => rest,
            _ => self.id.as_str(),
        }
    }

    /// Parse an `app:<id>` / `uri:<uri>` dedupe key back into an entity.
    /// Returns `None` for keys that name no entity.
    pub fn from_key(key: &str, display_name: &str) -> Option<Self> {
        if let Some(app_id) = key.strip_prefix("app:") {
            Some(Entity::new(key.to_owned(), EntityType::App, display_name).with("app_id", app_id))
        } else if let Some(uri) = key.strip_prefix("uri:") {
            let kind = if uri.ends_with('/') {
                EntityType::Folder
            } else {
                EntityType::File
            };
            Some(Entity::new(key.to_owned(), kind, display_name).with("uri", uri))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_become_entities() {
        let app = Entity::from_key("app:org.gnome.Calculator.desktop", "Calculator").unwrap();
        assert_eq!(app.entity_type, EntityType::App);
        assert_eq!(app.reference(), "app:org.gnome.Calculator.desktop");
        let file = Entity::from_key("uri:file:///x/y.pdf", "y.pdf").unwrap();
        assert_eq!(file.entity_type, EntityType::File);
        assert_eq!(file.reference(), "file:file:///x/y.pdf");
        let folder = Entity::from_key("uri:file:///x/", "x").unwrap();
        assert_eq!(folder.entity_type, EntityType::Folder);
        assert!(Entity::from_key("cmd:system.lock", "Lock").is_none());
    }

    #[test]
    fn unknown_ids_still_reference() {
        let e = Entity::new("weather:nairobi", EntityType::Weather, "Nairobi");
        assert_eq!(e.reference(), "weather:nairobi");
    }
}
