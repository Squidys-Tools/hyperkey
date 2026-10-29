//! Settings file persistence, mirroring the C# SettingsStore: the file lives
//! under %LOCALAPPDATA%\Hyperkey and writes go through a temporary sibling so a
//! crash mid-write cannot leave a truncated settings file behind.
use hyperkey_core::json;
use hyperkey_core::settings::HyperkeySettings;
use std::fs;
use std::io;
use std::path::PathBuf;

pub struct SettingsLoadResult {
    pub settings: HyperkeySettings,
    pub message: Option<String>,
}

pub struct SettingsStore {
    settings_path: PathBuf,
    temporary_settings_path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Self {
        let directory = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Hyperkey");
        let settings_path = directory.join("settings.json");
        let mut temporary = settings_path.clone().into_os_string();
        temporary.push(".tmp");
        Self {
            settings_path,
            temporary_settings_path: PathBuf::from(temporary),
        }
    }

    pub fn settings_path(&self) -> &PathBuf {
        &self.settings_path
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
                let result = json::parse(&text);
                SettingsLoadResult {
                    settings: result.settings,
                    message: if result.used_defaults {
                        Some(format!(
                            "The settings file could not be read: {}",
                            result.error.unwrap_or_default()
                        ))
                    } else {
                        None
                    },
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

fn try_delete(path: &PathBuf) {
    let _ = fs::remove_file(path);
}
