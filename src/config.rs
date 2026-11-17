//! Persistent application configuration.
//!
//! The configuration lives in `%APPDATA%\IdleShutdown\config.json`. Loading is
//! deliberately forgiving: a missing, unreadable, malformed, or semantically
//! invalid file falls back to the built-in defaults instead of preventing the
//! application from starting.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
const CONFIG_DIR_NAME: &str = "IdleShutdown";
const CONFIG_FILE_NAME: &str = "config.json";

/// Default shutdown delay: 30 minutes.
pub const DEFAULT_SHUTDOWN_SECONDS: u64 = 30 * 60;
/// Default reminder delay: 5 minutes.
pub const DEFAULT_REMINDER_SECONDS: u64 = 5 * 60;
/// Default countdown duration: 2 minutes.
pub const DEFAULT_COUNTDOWN_SECONDS: u64 = 2 * 60;

/// The user-configurable durations and auto-start preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Idle duration after which Windows is shut down.
    pub shutdown_seconds: u64,
    /// Idle duration after which the floating reminder is shown.
    pub reminder_seconds: u64,
    /// How long before shutdown the countdown starts.
    pub countdown_seconds: u64,
    /// Mirrors the current HKCU Run registration. The registry is authoritative
    /// at startup; this field is persisted so the file remains self-describing.
    pub auto_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shutdown_seconds: DEFAULT_SHUTDOWN_SECONDS,
            reminder_seconds: DEFAULT_REMINDER_SECONDS,
            countdown_seconds: DEFAULT_COUNTDOWN_SECONDS,
            auto_start: false,
        }
    }
}

impl Settings {
    /// Validate the relationships required by the product specification.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.shutdown_seconds == 0 {
            return Err(ValidationError::ShutdownNotPositive);
        }
        if self.reminder_seconds >= self.shutdown_seconds {
            return Err(ValidationError::ReminderNotBeforeShutdown);
        }
        if self.countdown_seconds == 0 {
            return Err(ValidationError::CountdownNotPositive);
        }
        if self.countdown_seconds > self.shutdown_seconds {
            return Err(ValidationError::CountdownExceedsShutdown);
        }
        Ok(())
    }
}

/// Human-readable validation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationError {
    ShutdownNotPositive,
    ReminderNotBeforeShutdown,
    CountdownNotPositive,
    CountdownExceedsShutdown,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ShutdownNotPositive => "关机时间必须大于 0。",
            Self::ReminderNotBeforeShutdown => "提醒时间必须早于关机时间。",
            Self::CountdownNotPositive => "倒计时时间必须大于 0。",
            Self::CountdownExceedsShutdown => "倒计时时间不能大于关机时间。",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ValidationError {}

/// Result of reading the configuration, including a non-fatal warning to show
/// in the settings window.
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    pub settings: Settings,
    pub warning: Option<String>,
    pub path: PathBuf,
}

/// Returns the configuration directory, using `%APPDATA%` on Windows and a
/// local fallback if the environment variable is unavailable.
pub fn config_dir() -> PathBuf {
    if let Some(app_data) = std::env::var_os("APPDATA") {
        PathBuf::from(app_data).join(CONFIG_DIR_NAME)
    } else if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        exe_dir.join(CONFIG_DIR_NAME)
    } else {
        std::env::temp_dir().join(CONFIG_DIR_NAME)
    }
}

/// Absolute path of `config.json`.
pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

/// Load the configuration from disk. Missing files are normal; every other
/// failure produces a warning and the defaults.
pub fn load() -> LoadedConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(raw) => match decode(&raw) {
            Ok(settings) => LoadedConfig {
                settings,
                warning: None,
                path,
            },
            Err(message) => {
                backup_invalid_config(&path);
                LoadedConfig {
                    settings: Settings::default(),
                    warning: Some(format!(
                        "{message} 已使用默认配置；原文件备份为 config.invalid.json。"
                    )),
                    path,
                }
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedConfig {
            settings: Settings::default(),
            warning: None,
            path,
        },
        Err(error) => LoadedConfig {
            settings: Settings::default(),
            warning: Some(format!(
                "无法读取配置文件（{error}），已使用默认配置。路径：{}",
                path.display()
            )),
            path,
        },
    }
}

/// Parse and validate the JSON representation. Kept separate for unit tests.
pub fn decode(raw: &str) -> Result<Settings, String> {
    let stored: StoredConfig =
        serde_json::from_str(raw).map_err(|error| format!("配置文件格式无效：{error}"))?;
    if stored.version != CONFIG_VERSION {
        return Err(format!(
            "配置文件版本 {} 不受支持（当前版本 {CONFIG_VERSION}）",
            stored.version
        ));
    }
    let settings = stored.into_settings();
    settings.validate().map_err(|error| error.to_string())?;
    Ok(settings)
}

fn backup_invalid_config(path: &Path) {
    if !path.is_file() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::copy(path, parent.join("config.invalid.json"));
    }
}

/// Write the settings as pretty JSON, creating the data directory if needed.
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建配置目录 {}：{error}", parent.display()))?;
    }

    let stored = StoredConfig::from_settings(settings);
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|error| format!("无法序列化配置：{error}"))?;

    // Write to a sibling temporary file first. If the destination exists,
    // std::fs::rename is not guaranteed to replace it on Windows, so fall back
    // to copying the fully written file over the destination.
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, json.as_bytes())
        .map_err(|error| format!("无法写入临时配置文件：{error}"))?;
    let result = fs::rename(&temp_path, path).or_else(|_| {
        fs::copy(&temp_path, path).map(|_| ())?;
        fs::remove_file(&temp_path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("配置已写入，但无法删除临时文件：{error}"),
            )
        })
    });
    result.map_err(|error| format!("无法保存配置到 {}：{error}", path.display()))
}

/// On-disk schema. Keeping the version explicit makes future migrations
/// possible without guessing about the format.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredConfig {
    version: u32,
    shutdown_seconds: u64,
    reminder_seconds: u64,
    countdown_seconds: u64,
    #[serde(default)]
    auto_start: bool,
}

impl StoredConfig {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            version: CONFIG_VERSION,
            shutdown_seconds: settings.shutdown_seconds,
            reminder_seconds: settings.reminder_seconds,
            countdown_seconds: settings.countdown_seconds,
            auto_start: settings.auto_start,
        }
    }

    fn into_settings(self) -> Settings {
        Settings {
            shutdown_seconds: self.shutdown_seconds,
            reminder_seconds: self.reminder_seconds,
            countdown_seconds: self.countdown_seconds,
            auto_start: self.auto_start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> &'static str {
        r#"{
            "version": 1,
            "shutdown_seconds": 1800,
            "reminder_seconds": 300,
            "countdown_seconds": 120,
            "auto_start": true
        }"#
    }

    #[test]
    fn decodes_valid_configuration() {
        let settings = decode(valid_json()).expect("valid config");
        assert_eq!(settings.shutdown_seconds, 1800);
        assert_eq!(settings.reminder_seconds, 300);
        assert_eq!(settings.countdown_seconds, 120);
        assert!(settings.auto_start);
    }

    #[test]
    fn missing_auto_start_defaults_to_false() {
        let raw = r#"{
            "version": 1,
            "shutdown_seconds": 60,
            "reminder_seconds": 10,
            "countdown_seconds": 10
        }"#;
        let settings = decode(raw).expect("valid config");
        assert!(!settings.auto_start);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(decode("{ nope").is_err());
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let raw = valid_json().replace("\"version\": 1", "\"version\": 2");
        let error = decode(&raw).expect_err("version must be checked");
        assert!(error.contains("版本"));
    }

    #[test]
    fn semantics_are_validated_on_load() {
        let raw = valid_json().replace("\"reminder_seconds\": 300", "\"reminder_seconds\": 1800");
        assert!(decode(&raw).is_err());
    }

    #[test]
    fn reminder_may_be_zero() {
        let settings = Settings {
            shutdown_seconds: 60,
            reminder_seconds: 0,
            countdown_seconds: 60,
            auto_start: false,
        };
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn countdown_can_equal_shutdown() {
        let settings = Settings {
            shutdown_seconds: 60,
            reminder_seconds: 10,
            countdown_seconds: 60,
            auto_start: false,
        };
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn invalid_relationships_are_rejected() {
        let base = Settings::default();
        assert_eq!(
            Settings {
                shutdown_seconds: 0,
                ..base.clone()
            }
            .validate(),
            Err(ValidationError::ShutdownNotPositive)
        );
        assert_eq!(
            Settings {
                reminder_seconds: base.shutdown_seconds,
                ..base.clone()
            }
            .validate(),
            Err(ValidationError::ReminderNotBeforeShutdown)
        );
        assert_eq!(
            Settings {
                countdown_seconds: 0,
                ..base.clone()
            }
            .validate(),
            Err(ValidationError::CountdownNotPositive)
        );
        assert_eq!(
            Settings {
                countdown_seconds: base.shutdown_seconds + 1,
                ..base
            }
            .validate(),
            Err(ValidationError::CountdownExceedsShutdown)
        );
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("idle-shutdown-config-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let settings = Settings {
            shutdown_seconds: 120,
            reminder_seconds: 30,
            countdown_seconds: 15,
            auto_start: true,
        };
        save(&path, &settings).expect("save");
        let raw = fs::read_to_string(&path).expect("read");
        let decoded = decode(&raw).expect("decode");
        assert_eq!(decoded, settings);
        let _ = fs::remove_dir_all(&dir);
    }
}
