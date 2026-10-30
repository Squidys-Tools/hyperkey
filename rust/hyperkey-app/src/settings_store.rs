//! Settings file persistence, mirroring the C# SettingsStore: the file lives
//! under %LOCALAPPDATA%\Hyperkey and writes go through a temporary sibling so a
//! crash mid-write cannot leave a truncated settings file behind.
//!
//! A document the tolerant parser cannot read at all (`ParseOutcome::used_defaults`)
//! is quarantined next to the settings file as `settings.json.bak` before
//! defaults are applied, so a corrupt file never silently becomes a reset.
//! Per-value repairs are surfaced to the caller so the settings window can
//! show them instead of dropping them quietly.
use hyperkey_core::json;
use hyperkey_core::settings::HyperkeySettings;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct SettingsLoadResult {
    pub settings: HyperkeySettings,
    /// Banner text for the settings window: the fatal error and quarantine
    /// outcome on an unreadable file, the repair report on a repaired file,
    /// or `None` on a clean load.
    pub message: Option<String>,
}

pub struct SettingsStore {
    settings_path: PathBuf,
    temporary_settings_path: PathBuf,
    backup_settings_path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Self {
        Self::at(
            dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Hyperkey"),
        )
    }

    /// Builds a store rooted at an explicit directory. `new` uses the real
    /// per-user data directory; tests pass a temporary directory instead so
    /// nothing ever touches the real %LOCALAPPDATA%\Hyperkey.
    pub fn at(directory: impl Into<PathBuf>) -> Self {
        let settings_path = directory.into().join("settings.json");
        let mut temporary = settings_path.clone().into_os_string();
        temporary.push(".tmp");
        let mut backup = settings_path.clone().into_os_string();
        backup.push(".bak");
        Self {
            settings_path,
            temporary_settings_path: PathBuf::from(temporary),
            backup_settings_path: PathBuf::from(backup),
        }
    }

    pub fn load(&self) -> SettingsLoadResult {
        try_delete(&self.temporary_settings_path);

        if !self.settings_path.exists() {
            return SettingsLoadResult {
                settings: HyperkeySettings::defaults(),
                message: None,
            };
        }

        match fs::read_to_string(&self.settings_path) {
            Ok(text) => {
                let outcome = json::parse(&text);
                if outcome.used_defaults {
                    self.quarantine(outcome.fatal_error.as_deref().unwrap_or_default())
                } else {
                    SettingsLoadResult {
                        settings: outcome.settings,
                        message: if outcome.repaired {
                            Some(format!(
                                "Some settings were repaired: {}",
                                outcome.repairs.join("; ")
                            ))
                        } else {
                            None
                        },
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                SettingsLoadResult {
                    settings: HyperkeySettings::defaults(),
                    message: Some(format!(
                        "Hyperkey could not access its settings file: {error}"
                    )),
                }
            }
            Err(error) => SettingsLoadResult {
                settings: HyperkeySettings::defaults(),
                message: Some(format!("The settings file could not be read: {error}")),
            },
        }
    }

    /// Moves the unreadable settings file aside to `settings.json.bak` and
    /// reports defaults. The move is best-effort: defaults apply either way,
    /// and the message says which outcome the user got.
    fn quarantine(&self, fatal_error: &str) -> SettingsLoadResult {
        let quarantined = match self.replace_file(&self.settings_path, &self.backup_settings_path)
        {
            Ok(()) => Some("settings.json.bak"),
            Err(_) => None,
        };

        let message = match quarantined {
            Some(name) => format!(
                "Hyperkey loaded safe defaults. The settings file could not be read: \
                 {fatal_error}. The original file was moved to {name}."
            ),
            None => format!(
                "Hyperkey loaded safe defaults. The settings file could not be read: \
                 {fatal_error}. It could not be moved aside, so it will be replaced \
                 the next time settings are saved."
            ),
        };

        SettingsLoadResult {
            settings: HyperkeySettings::defaults(),
            message: Some(message),
        }
    }

    fn replace_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        try_delete(to);
        fs::rename(from, to)
    }

    pub fn save(&self, settings: &HyperkeySettings) -> Result<(), String> {
        let Some(directory) = self.settings_path.parent() else {
            return Err("The settings directory could not be resolved.".to_string());
        };

        let result = (|| -> io::Result<()> {
            fs::create_dir_all(directory)?;
            fs::write(
                &self.temporary_settings_path,
                json::serialize(settings),
            )?;
            fs::rename(&self.temporary_settings_path, &self.settings_path)
        })();

        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                try_delete(&self.temporary_settings_path);
                if error.kind() == io::ErrorKind::PermissionDenied {
                    Err(format!("Hyperkey could not write its settings: {error}"))
                } else {
                    Err(format!("Settings could not be saved: {error}"))
                }
            }
        }
    }
}

fn try_delete(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_directory(name: &str) -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "hyperkey-store-test-{}-{name}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn paths_live_under_the_store_directory() {
        let directory = test_directory("paths");
        let store = SettingsStore::at(&directory);

        assert_eq!(directory.join("settings.json"), store.settings_path);
        assert_eq!(
            directory.join("settings.json.tmp"),
            store.temporary_settings_path
        );
        assert_eq!(
            directory.join("settings.json.bak"),
            store.backup_settings_path
        );
    }

    #[test]
    fn missing_file_loads_defaults_without_a_message() {
        let store = SettingsStore::at(test_directory("missing"));

        let result = store.load();

        assert_eq!(HyperkeySettings::defaults(), result.settings);
        assert!(result.message.is_none());
    }

    #[test]
    fn unreadable_file_is_quarantined_and_defaults_load() {
        let directory = test_directory("quarantine");
        fs::create_dir_all(&directory).unwrap();
        let store = SettingsStore::at(&directory);
        fs::write(&store.settings_path, "{{{ not json").unwrap();

        let result = store.load();

        assert_eq!(HyperkeySettings::defaults(), result.settings);
        let message = result.message.expect("a fatal load reports a message");
        assert!(message.contains("settings.json.bak"), "message: {message}");
        // The original content is preserved under .bak, not destroyed.
        assert_eq!(
            "{{{ not json",
            fs::read_to_string(directory.join("settings.json.bak")).unwrap()
        );
        assert!(!store.settings_path.exists());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn quarantine_replaces_an_older_backup() {
        let directory = test_directory("quarantine-replace");
        fs::create_dir_all(&directory).unwrap();
        let store = SettingsStore::at(&directory);
        fs::write(&store.settings_path, "{ invalid").unwrap();
        fs::write(directory.join("settings.json.bak"), "older backup").unwrap();

        let result = store.load();

        assert_eq!(HyperkeySettings::defaults(), result.settings);
        assert_eq!(
            "{ invalid",
            fs::read_to_string(directory.join("settings.json.bak")).unwrap()
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn repaired_values_are_surfaced_without_quarantine() {
        let directory = test_directory("repairs");
        fs::create_dir_all(&directory).unwrap();
        let store = SettingsStore::at(&directory);
        fs::write(
            &store.settings_path,
            r#"{"schemaVersion":2,"enabled":false,"trigger":"NotAKey","outputModifiers":["Control"],"launchAtStartup":false,"launchToTray":true}"#,
        )
        .unwrap();

        let result = store.load();

        assert_eq!(TriggerKey::CAPS_LOCK, result.settings.trigger());
        assert!(!result.settings.enabled());
        let message = result
            .message
            .expect("a repaired load reports the repairs");
        assert!(message.contains("trigger"), "message: {message}");
        // Repairs stay in the file until the user saves; nothing is moved.
        assert!(store.settings_path.exists());
        assert!(!directory.join("settings.json.bak").exists());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn clean_file_loads_without_a_message() {
        let directory = test_directory("clean");
        fs::create_dir_all(&directory).unwrap();
        let store = SettingsStore::at(&directory);
        fs::write(&store.settings_path, json::serialize(&HyperkeySettings::defaults()))
            .unwrap();

        let result = store.load();

        assert_eq!(HyperkeySettings::defaults(), result.settings);
        assert!(result.message.is_none());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn save_then_load_round_trips() {
        let directory = test_directory("roundtrip");
        let store = SettingsStore::at(&directory);
        let settings = HyperkeySettings::defaults().with_enabled(false);

        store.save(&settings).unwrap();
        let result = store.load();

        assert_eq!(settings, result.settings);
        assert!(result.message.is_none());
        let _ = fs::remove_dir_all(&directory);
    }

    use hyperkey_core::keys::TriggerKey;
}
