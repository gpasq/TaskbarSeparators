//! The saved list of separators.

use crate::icon::Style;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Separator {
    pub id: String,
    /// Shown as the tooltip and in the taskbar right-click menu.
    pub name: String,
    pub style: Style,
    /// Whether the app keeps a live taskbar button for it. Pinned separators stay in the
    /// taskbar on their own, so users can turn this off for them.
    pub live: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub separators: Vec<Separator>,
}

impl Store {
    pub fn load(path: &Path) -> Store {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, path)
    }
}

/// Short unique id, safe for file names and AppUserModelIDs.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:x}{:02x}",
        (nanos / 1000) as u64,
        COUNTER.fetch_add(1, Ordering::Relaxed) & 0xff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let p = std::env::temp_dir().join("taskbar-separators-store-test.json");
        let store = Store {
            separators: vec![Separator {
                id: new_id(),
                name: "Separator".into(),
                style: Style::default(),
                live: true,
            }],
        };
        store.save(&p).unwrap();
        let back = Store::load(&p);
        assert_eq!(back.separators.len(), 1);
        assert_eq!(back.separators[0].style, Style::default());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(), new_id());
    }
}
