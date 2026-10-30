//! Per-user startup registration, mirroring the C# StartupRegistrationService:
//! an HKCU `Run` value named `Hyperkey` that relaunches the current executable
//! with `--tray` so login starts the daemon without opening the window.
//!
//! The command string is built by [`run_command`], kept pure so tests never
//! touch the registry.
use std::iter::once;
use std::path::Path;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::core::{HRESULT, PCWSTR};

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Hyperkey";

/// Writes or removes the `Run` value. Returns the failure text to surface in
/// the diagnostics banner, matching the C# error strings.
pub fn apply(enabled: bool) -> Result<(), String> {
    if !enabled {
        return delete_value();
    }

    let executable = std::env::current_exe().map_err(|error| {
        format!("Launch at login could not be updated: {error}")
    })?;
    set_value(&run_command(&executable))
}

/// The registry value payload: the quoted executable path followed by the
/// `--tray` argument. Quotes inside the path are backslash-escaped, as in the
/// C# implementation.
pub fn run_command(executable: &Path) -> String {
    let escaped = executable.to_string_lossy().replace('"', "\\\"");
    format!("\"{escaped}\" --tray")
}

fn set_value(command: &str) -> Result<(), String> {
    let subkey = wide(RUN_KEY_PATH);
    let value = wide(VALUE_NAME);
    let mut command_wide: Vec<u16> = command.encode_utf16().chain(once(0)).collect();
    // RegSetKeyValueW reads the buffer only for the call duration.
    let bytes = (command_wide.len() * size_of::<u16>()) as u32;

    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            REG_SZ.0,
            Some(command_wide.as_mut_ptr().cast()),
            bytes,
        )
    }
    .ok()
    .map_err(|error| format!("Launch at login could not be updated: {error}"))
}

fn delete_value() -> Result<(), String> {
    let subkey = wide(RUN_KEY_PATH);
    let value = wide(VALUE_NAME);

    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), PCWSTR(value.as_ptr())) }
        .ok()
        .or_else(|error| {
            // A missing value is already the desired end state.
            if error.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(|error| format!("Launch at login could not be updated: {error}"))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn run_command_quotes_the_path_and_appends_tray() {
        let command = run_command(&PathBuf::from(r"C:\Program Files\Hyperkey\Hyperkey.App.exe"));

        assert_eq!(
            r#""C:\Program Files\Hyperkey\Hyperkey.App.exe" --tray"#,
            command
        );
    }

    #[test]
    fn run_command_escapes_embedded_quotes() {
        let command = run_command(&PathBuf::from(r#"C:\odd"path\app.exe"#));

        assert_eq!(r#""C:\odd\"path\app.exe" --tray"#, command);
    }
}
