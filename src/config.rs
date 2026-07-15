use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::net::SocketAddr;
use winit::keyboard::KeyCode;

const MIN_RENDER_DISTANCE: i32 = 2;
const MAX_RENDER_DISTANCE: i32 = 32;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsQuality {
    #[default]
    Regular,
    Vibrant,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct KeyBindings {
    pub forward: String,
    pub back: String,
    pub left: String,
    pub right: String,
    pub jump: String,
    pub sneak: String,
    pub sprint: String,
    pub inventory: String,
    pub drop_item: String,
    pub chat: String,
    pub command: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: "KeyW".to_string(),
            back: "KeyS".to_string(),
            left: "KeyA".to_string(),
            right: "KeyD".to_string(),
            jump: "Space".to_string(),
            sneak: "ShiftLeft".to_string(),
            sprint: "ControlLeft".to_string(),
            inventory: "KeyE".to_string(),
            drop_item: "KeyQ".to_string(),
            chat: "KeyT".to_string(),
            command: "Slash".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedKeyBindings {
    pub forward: KeyCode,
    pub back: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub jump: KeyCode,
    pub sneak: KeyCode,
    pub sprint: KeyCode,
    pub inventory: KeyCode,
    pub drop_item: KeyCode,
    pub chat: KeyCode,
    pub command: KeyCode,
}

impl KeyBindings {
    pub fn resolve(&self) -> Result<ResolvedKeyBindings, ConfigError> {
        Ok(ResolvedKeyBindings {
            forward: parse_key("forward", &self.forward)?,
            back: parse_key("back", &self.back)?,
            left: parse_key("left", &self.left)?,
            right: parse_key("right", &self.right)?,
            jump: parse_key("jump", &self.jump)?,
            sneak: parse_key("sneak", &self.sneak)?,
            sprint: parse_key("sprint", &self.sprint)?,
            inventory: parse_key("inventory", &self.inventory)?,
            drop_item: parse_key("drop_item", &self.drop_item)?,
            chat: parse_key("chat", &self.chat)?,
            command: parse_key("command", &self.command)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub seed: Option<u64>,
    pub world_dir: PathBuf,
    pub server: Option<SocketAddr>,
    pub username: String,
    pub render_distance: i32,
    pub graphics: GraphicsQuality,
    pub keybindings: KeyBindings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            seed: None,
            world_dir: PathBuf::from("world"),
            server: None,
            username: "Player".to_string(),
            render_distance: 6,
            graphics: GraphicsQuality::Regular,
            keybindings: KeyBindings::default(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    HelpRequested,
    Message(String),
    Io { path: PathBuf, source: std::io::Error },
    Json { path: PathBuf, source: serde_json::Error },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HelpRequested => write!(f, "{}", usage()),
            Self::Message(message) => write!(f, "{message}\n\n{}", usage()),
            Self::Io { path, source } => write!(f, "failed to read configuration {}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "invalid configuration {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

fn default_config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("VIBECRAFT_CONFIG") {
        return PathBuf::from(path);
    }
    let base = if cfg!(target_os = "linux") {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME").unwrap_or_default();
                PathBuf::from(home).join(".config")
            })
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(".")
    };
    base.join("vibecraft").join("vibecraft.json")
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_args(std::env::args().skip(1))
    }

    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let args: Vec<_> = args.into_iter().collect();
        let mut config_path = default_config_path();

        for index in 0..args.len() {
            if args[index] == "--config" {
                config_path = PathBuf::from(argument(&args, index, "--config")?);
            }
            if args[index] == "--help" || args[index] == "-h" {
                return Err(ConfigError::HelpRequested);
            }
        }

        let mut config = if config_path.exists() {
            Self::load(&config_path)?
        } else {
            Self::default()
        };

        let mut index = 0;
        while index < args.len() {
            let option = &args[index];
            match option.as_str() {
                "--config" => index += 1,
                "--seed" => {
                    config.seed = Some(parse_value("--seed", argument(&args, index, option)?)?);
                    index += 1;
                }
                "--world-dir" => {
                    config.world_dir = PathBuf::from(argument(&args, index, option)?);
                    index += 1;
                }
                "--server" => {
                    config.server = Some(parse_value("--server", argument(&args, index, option)?)?);
                    index += 1;
                }
                "--username" => {
                    config.username = argument(&args, index, option)?.to_string();
                    index += 1;
                }
                "--render-distance" => {
                    config.render_distance = parse_value("--render-distance", argument(&args, index, option)?)?;
                    index += 1;
                }
                "--graphics" => {
                    config.graphics = match argument(&args, index, option)?.to_ascii_lowercase().as_str() {
                        "regular" => GraphicsQuality::Regular,
                        "vibrant" => GraphicsQuality::Vibrant,
                        value => return Err(ConfigError::Message(format!("invalid --graphics value `{value}`; expected regular or vibrant"))),
                    };
                    index += 1;
                }
                "--keybind" => {
                    let binding = argument(&args, index, option)?;
                    let (action, key) = binding.split_once('=').ok_or_else(|| {
                        ConfigError::Message("--keybind must be ACTION=KEY, for example forward=KeyW".to_string())
                    })?;
                    config.keybindings.set(action, key)?;
                    index += 1;
                }
                "--help" | "-h" => unreachable!("help was handled before loading configuration"),
                value if value.starts_with('-') => return Err(ConfigError::Message(format!("unknown option `{value}`"))),
                value => return Err(ConfigError::Message(format!("unexpected argument `{value}`"))),
            }
            index += 1;
        }
        config.validate()?;
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_str(&contents).map_err(|source| ConfigError::Json {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(MIN_RENDER_DISTANCE..=MAX_RENDER_DISTANCE).contains(&self.render_distance) {
            return Err(ConfigError::Message(format!(
                "render_distance must be between {MIN_RENDER_DISTANCE} and {MAX_RENDER_DISTANCE}"
            )));
        }
        if self.username.is_empty() || self.username.len() > 16 || self.username.chars().any(char::is_control) {
            return Err(ConfigError::Message("username must be 1-16 bytes and contain no control characters".to_string()));
        }
        self.keybindings.resolve()?;
        Ok(())
    }

    pub fn resolved_seed(&self) -> u64 {
        self.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0)
        })
    }
}

impl KeyBindings {
    fn set(&mut self, action: &str, key: &str) -> Result<(), ConfigError> {
        parse_key(action, key)?;
        let slot = match action {
            "forward" => &mut self.forward,
            "back" => &mut self.back,
            "left" => &mut self.left,
            "right" => &mut self.right,
            "jump" => &mut self.jump,
            "sneak" => &mut self.sneak,
            "sprint" => &mut self.sprint,
            "inventory" => &mut self.inventory,
            "drop_item" => &mut self.drop_item,
            "chat" => &mut self.chat,
            "command" => &mut self.command,
            _ => return Err(ConfigError::Message(format!("unknown keybinding action `{action}`"))),
        };
        *slot = key.to_string();
        Ok(())
    }
}

fn argument<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, ConfigError> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| ConfigError::Message(format!("missing value for `{option}`")))
}

fn parse_value<T: std::str::FromStr>(option: &str, value: &str) -> Result<T, ConfigError> {
    value.parse().map_err(|_| ConfigError::Message(format!("invalid {option} value `{value}`")))
}

fn parse_key(action: &str, key: &str) -> Result<KeyCode, ConfigError> {
    let code = match key {
        "KeyW" => KeyCode::KeyW, "KeyA" => KeyCode::KeyA, "KeyS" => KeyCode::KeyS, "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE, "KeyQ" => KeyCode::KeyQ, "KeyT" => KeyCode::KeyT,
        "Space" => KeyCode::Space,
        "ShiftLeft" => KeyCode::ShiftLeft, "ShiftRight" => KeyCode::ShiftRight,
        "ControlLeft" => KeyCode::ControlLeft, "ControlRight" => KeyCode::ControlRight,
        "Slash" => KeyCode::Slash,
        _ => return Err(ConfigError::Message(format!("unsupported key `{key}` for `{action}`"))),
    };
    Ok(code)
}

pub fn usage() -> String {
    format!("Usage: vibecraft [--config PATH] [--seed U64] [--world-dir PATH] [--server IP:PORT] [--username NAME] [--render-distance 2..32] [--graphics regular|vibrant] [--keybind ACTION=KEY]\n\nConfiguration is JSON (default: {}). Supported actions: forward, back, left, right, jump, sneak, sprint, inventory, drop_item, chat, command.", default_config_path().display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_settings_override_defaults() {
        let config = AppConfig::from_args([
            "--seed".into(), "42".into(), "--world-dir".into(), "fixture-world".into(),
            "--server".into(), "127.0.0.1:25565".into(), "--username".into(), "Alex".into(),
            "--render-distance".into(), "12".into(), "--graphics".into(), "vibrant".into(),
            "--keybind".into(), "forward=KeyD".into(),
        ]).unwrap();
        assert_eq!(config.seed, Some(42));
        assert_eq!(config.world_dir, PathBuf::from("fixture-world"));
        assert_eq!(config.server, Some("127.0.0.1:25565".parse().unwrap()));
        assert_eq!(config.username, "Alex");
        assert_eq!(config.render_distance, 12);
        assert_eq!(config.graphics, GraphicsQuality::Vibrant);
        assert_eq!(config.keybindings.resolve().unwrap().forward, KeyCode::KeyD);
    }

    #[test]
    fn invalid_settings_are_rejected() {
        assert!(AppConfig::from_args(["--render-distance".into(), "1".into()]).is_err());
        assert!(AppConfig::from_args(["--keybind".into(), "jump=F24".into()]).is_err());
    }
}
