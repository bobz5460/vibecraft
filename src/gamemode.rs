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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub const fn name(&self) -> &'static str {
        match self {
            Difficulty::Peaceful => "Peaceful",
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    }

    /// Damage multiplier applied to incoming damage.
    /// Peaceful = 0 (no mob damage), Easy = 0.5, Normal = 1.0, Hard = 1.5
    pub const fn damage_multiplier(&self) -> f32 {
        match self {
            Difficulty::Peaceful => 0.0,
            Difficulty::Easy => 0.5,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 1.5,
        }
    }

    /// Whether natural health regeneration is allowed.
    /// Peaceful: always regens, Easy/Normal: regens normally, Hard: no natural regen
    pub const fn natural_regen_allowed(&self) -> bool {
        !matches!(self, Difficulty::Hard)
    }

    /// Whether starvation damage can occur.
    pub const fn starvation_enabled(&self) -> bool {
        !matches!(self, Difficulty::Peaceful)
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "peaceful" | "p" | "0" => Some(Difficulty::Peaceful),
            "easy" | "e" | "1" => Some(Difficulty::Easy),
            "normal" | "n" | "2" => Some(Difficulty::Normal),
            "hard" | "h" | "3" => Some(Difficulty::Hard),
            _ => None,
        }
    }
}
