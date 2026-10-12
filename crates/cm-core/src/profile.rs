use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::UnixMillis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(pub Uuid);

impl ProfileId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// The id of the built-in profile that maps to the stock `%APPDATA%\Claude` folder.
    pub const DEFAULT: ProfileId = ProfileId(Uuid::nil());

    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.simple())
    }
}

impl std::str::FromStr for ProfileId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(ProfileId)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileIcon {
    #[default]
    Sparkle,
    Briefcase,
    Home,
    Code,
    Rocket,
    Star,
    Heart,
    Globe,
}

impl ProfileIcon {
    pub const ALL: [ProfileIcon; 8] = [
        Self::Sparkle,
        Self::Briefcase,
        Self::Home,
        Self::Code,
        Self::Rocket,
        Self::Star,
        Self::Heart,
        Self::Globe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sparkle => "Sparkle",
            Self::Briefcase => "Briefcase",
            Self::Home => "Home",
            Self::Code => "Code",
            Self::Rocket => "Rocket",
            Self::Star => "Star",
            Self::Heart => "Heart",
            Self::Globe => "Globe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileColor {
    #[default]
    Coral,
    Sky,
    Sage,
    Ochre,
    Plum,
    Stone,
}

impl ProfileColor {
    pub const ALL: [ProfileColor; 6] = [
        Self::Coral,
        Self::Sky,
        Self::Sage,
        Self::Ochre,
        Self::Plum,
        Self::Stone,
    ];

    /// 0xRRGGBB, the soft Meadow profile palette.
    pub fn rgb(self) -> u32 {
        match self {
            Self::Coral => 0xE07A5F,
            Self::Sky => 0x5E93BC,
            Self::Sage => 0x6F9C78,
            Self::Ochre => 0xCF9A3E,
            Self::Plum => 0x9A6B96,
            Self::Stone => 0x857B70,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Coral => "Coral",
            Self::Sky => "Sky",
            Self::Sage => "Sage",
            Self::Ochre => "Ochre",
            Self::Plum => "Plum",
            Self::Stone => "Stone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    /// When true, `CLAUDE_CONFIG_DIR` is set for the Desktop instance and profile terminals.
    pub enabled: bool,
    pub config_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub icon: ProfileIcon,
    #[serde(default)]
    pub color: ProfileColor,
    /// Accelerator string understood by `global-hotkey`, e.g. `"Ctrl+Alt+1"`.
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub auto_restart: bool,
    /// `None` means the stock data folder (launch without `--user-data-dir`).
    pub user_data_dir: Option<PathBuf>,
    pub claude_code: ClaudeCodeConfig,
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub created_at: UnixMillis,
    #[serde(default)]
    pub last_launched_at: Option<UnixMillis>,
    /// Last `lastKnownAccountUuid` seen in this profile's `config.json`.
    #[serde(default)]
    pub cached_account_uuid: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Profile {
    pub fn is_default(&self) -> bool {
        self.id.is_default()
    }
}
