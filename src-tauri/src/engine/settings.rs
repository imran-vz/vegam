//! Settings persistence and the Display Name default.
//!
//! Display Name is cosmetic (ADR 0017): a random two-word name on first run,
//! user-editable, never identity/authentication/authorization/trust, never
//! placed in tickets or sent on the wire.

use std::path::Path;

use anyhow::{Context, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::engine::persist::write_atomic;
use crate::engine::types::Settings;

#[derive(Serialize, Deserialize)]
struct SettingsFile {
    schema_version: u32,
    display_name: String,
    #[serde(default)]
    analytics_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    analytics_id: Option<String>,
}

const SCHEMA_VERSION: u32 = 1;

const ADJECTIVES: &[&str] = &[
    "amber", "bold", "brisk", "calm", "clever", "cobalt", "coral", "crisp", "daring", "deft",
    "eager", "fleet", "gentle", "glad", "golden", "hardy", "hazel", "ivory", "jade", "keen",
    "lively", "lucid", "mellow", "merry", "noble", "nimble", "olive", "opal", "plucky", "proud",
    "quick", "quiet", "rapid", "rosy", "rust", "sage", "scarlet", "silver", "spry", "steady",
    "sunny", "swift", "teal", "tidy", "vivid", "warm", "wise", "zesty",
];

const NOUNS: &[&str] = &[
    "anchor", "aspen", "badger", "beacon", "birch", "breeze", "brook", "canyon", "cedar", "cliff",
    "comet", "cove", "crane", "delta", "dune", "ember", "falcon", "fern", "fjord", "gale",
    "glacier", "grove", "harbor", "heron", "island", "lagoon", "lantern", "linden", "maple",
    "meadow", "mesa", "otter", "pebble", "pine", "prairie", "raven", "reef", "ridge", "river",
    "sparrow", "summit", "thicket", "tide", "trail", "tundra", "valley", "willow", "wren",
];

pub fn generate_display_name<R: Rng>(rng: &mut R) -> String {
    let adjective = ADJECTIVES[rng.random_range(0..ADJECTIVES.len())];
    let noun = NOUNS[rng.random_range(0..NOUNS.len())];
    format!("{adjective}-{noun}")
}

pub fn load_or_create(path: &Path) -> Result<Settings> {
    if path.exists() {
        let raw = std::fs::read_to_string(path).context("reading settings")?;
        let file: SettingsFile = serde_json::from_str(&raw).context("parsing settings")?;
        Ok(Settings {
            display_name: file.display_name,
            analytics_enabled: file.analytics_enabled,
            analytics_id: file.analytics_id,
        })
    } else {
        let settings = Settings {
            display_name: generate_display_name(&mut rand::rng()),
            analytics_enabled: false,
            analytics_id: None,
        };
        save(path, &settings)?;
        Ok(settings)
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let file = SettingsFile {
        schema_version: SCHEMA_VERSION,
        display_name: settings.display_name.clone(),
        analytics_enabled: settings.analytics_enabled,
        analytics_id: settings.analytics_id.clone(),
    };
    let json = serde_json::to_string_pretty(&file)?;
    write_atomic(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn name_is_two_words() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let name = generate_display_name(&mut rng);
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(ADJECTIVES.contains(&parts[0]));
        assert!(NOUNS.contains(&parts[1]));
    }

    #[test]
    fn load_save_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("vegam-settings-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let created = load_or_create(&path).unwrap();
        assert!(!created.display_name.is_empty());

        let edited = Settings {
            display_name: "my laptop".to_string(),
            analytics_enabled: false,
            analytics_id: None,
        };
        save(&path, &edited).unwrap();
        let loaded = load_or_create(&path).unwrap();
        assert_eq!(loaded.display_name, "my laptop");

        std::fs::remove_dir_all(&dir).ok();
    }
}
