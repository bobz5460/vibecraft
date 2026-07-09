#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub const fn name(&self) -> &'static str {
        match self {
            GameMode::Survival => "Survival",
            GameMode::Creative => "Creative",
            GameMode::Adventure => "Adventure",
            GameMode::Spectator => "Spectator",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "survival" | "s" | "0" => Some(GameMode::Survival),
            "creative" | "c" | "1" => Some(GameMode::Creative),
            "adventure" | "a" | "2" => Some(GameMode::Adventure),
            "spectator" | "sp" | "3" => Some(GameMode::Spectator),
            _ => None,
        }
    }

    pub fn can_fly(&self) -> bool {
        matches!(self, GameMode::Creative | GameMode::Spectator)
    }

    pub fn has_gravity(&self) -> bool {
        matches!(self, GameMode::Survival | GameMode::Adventure)
    }

    pub fn can_break(&self) -> bool {
        matches!(self, GameMode::Survival | GameMode::Creative)
    }

    pub fn can_place(&self) -> bool {
        matches!(self, GameMode::Survival | GameMode::Creative)
    }

    pub fn instant_break(&self) -> bool {
        matches!(self, GameMode::Creative)
    }

    pub fn takes_damage(&self) -> bool {
        matches!(self, GameMode::Survival | GameMode::Adventure)
    }

    pub fn is_spectator(&self) -> bool {
        *self == GameMode::Spectator
    }
}
