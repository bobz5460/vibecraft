#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusEffect {
    Speed,
    Slowness,
    Haste,
    MiningFatigue,
    Strength,
    JumpBoost,
    Regeneration,
    Resistance,
    FireResistance,
    WaterBreathing,
    NightVision,
    Invisibility,
    Absorption,
    SlowFalling,
    DolphinGrace,
    Weakness,
    Poison,
    Wither,
    Hunger,
    Nausea,
    Blindness,
    Levitation,
    Darkness,
    InstantHealth,
    InstantDamage,
    HealthBoost,
    SaturationEffect,
    FatalPoison,
    BadOmen,
    HeroOfTheVillage,
    WindCharged,
    Infested,
    Oozing,
    Weaving,
}

impl StatusEffect {
    pub fn name(&self) -> &'static str {
        match self {
            StatusEffect::Speed => "Speed",
            StatusEffect::Slowness => "Slowness",
            StatusEffect::Haste => "Haste",
            StatusEffect::MiningFatigue => "MiningFatigue",
            StatusEffect::Strength => "Strength",
            StatusEffect::JumpBoost => "JumpBoost",
            StatusEffect::Regeneration => "Regen",
            StatusEffect::Resistance => "Resistance",
            StatusEffect::FireResistance => "FireResist",
            StatusEffect::WaterBreathing => "WaterBreath",
            StatusEffect::NightVision => "NightVision",
            StatusEffect::Invisibility => "Invisible",
            StatusEffect::Absorption => "Absorption",
            StatusEffect::SlowFalling => "SlowFall",
            StatusEffect::DolphinGrace => "DolphinGrace",
            StatusEffect::Weakness => "Weakness",
            StatusEffect::Poison => "Poison",
            StatusEffect::Wither => "Wither",
            StatusEffect::Hunger => "Hunger",
            StatusEffect::Nausea => "Nausea",
            StatusEffect::Blindness => "Blindness",
            StatusEffect::Levitation => "Levitation",
            StatusEffect::Darkness => "Darkness",
            StatusEffect::InstantHealth => "InstantHealth",
            StatusEffect::InstantDamage => "InstantDamage",
            StatusEffect::HealthBoost => "HealthBoost",
            StatusEffect::SaturationEffect => "Saturation",
            StatusEffect::FatalPoison => "FatalPoison",
            StatusEffect::BadOmen => "BadOmen",
            StatusEffect::HeroOfTheVillage => "HeroOfTheVillage",
            StatusEffect::WindCharged => "WindCharged",
            StatusEffect::Infested => "Infested",
            StatusEffect::Oozing => "Oozing",
            StatusEffect::Weaving => "Weaving",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusEffectInstance {
    pub effect: StatusEffect,
    pub duration: f32,
    pub amplifier: u32,
}

impl StatusEffectInstance {
    pub fn new(effect: StatusEffect, duration: f32, amplifier: u32) -> Self {
        StatusEffectInstance {
            effect,
            duration,
            amplifier,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.duration <= 0.0
    }

    pub fn tick(&mut self, dt: f32) {
        self.duration = (self.duration - dt).max(0.0);
    }

    pub fn level(&self) -> f32 {
        (self.amplifier + 1) as f32
    }
}

#[derive(Clone, Debug)]
pub struct EffectManager {
    pub effects: Vec<StatusEffectInstance>,
}

impl EffectManager {
    pub fn new() -> Self {
        EffectManager {
            effects: Vec::with_capacity(8),
        }
    }

    pub fn apply(&mut self, effect: StatusEffect, duration: f32, amplifier: u32) {
        for existing in &mut self.effects {
            if existing.effect == effect {
                if existing.amplifier < amplifier {
                    existing.amplifier = amplifier;
                    existing.duration = duration;
                } else if existing.amplifier == amplifier {
                    existing.duration = existing.duration.max(duration);
                }
                return;
            }
        }
        self.effects
            .push(StatusEffectInstance::new(effect, duration, amplifier));
    }

    pub fn remove(&mut self, effect: StatusEffect) {
        self.effects.retain(|e| e.effect != effect);
    }

    pub fn has(&self, effect: StatusEffect) -> bool {
        self.effects.iter().any(|e| e.effect == effect)
    }

    pub fn get_amplifier(&self, effect: StatusEffect) -> Option<u32> {
        self.effects
            .iter()
            .find(|e| e.effect == effect)
            .map(|e| e.amplifier)
    }

    pub fn get_level(&self, effect: StatusEffect) -> f32 {
        self.effects
            .iter()
            .find(|e| e.effect == effect)
            .map(|e| (e.amplifier + 1) as f32)
            .unwrap_or(0.0)
    }

    pub fn tick(&mut self, dt: f32) {
        for effect in &mut self.effects {
            effect.tick(dt);
        }
        self.effects.retain(|e| !e.is_expired());
    }

    pub fn clear(&mut self) {
        self.effects.clear();
    }

    pub fn speed_multiplier(&self) -> f32 {
        let speed = self.get_level(StatusEffect::Speed);
        let slow = self.get_level(StatusEffect::Slowness);
        let grace = if self.has(StatusEffect::DolphinGrace) {
            1.5
        } else {
            1.0
        };
        grace * (1.0 + 0.2 * speed) / (1.0 + 0.15 * slow)
    }

    pub fn jump_multiplier(&self) -> f32 {
        let boost = self.get_level(StatusEffect::JumpBoost);
        1.0 + 0.1 * boost
    }

    pub fn gravity_multiplier(&self) -> f32 {
        if self.has(StatusEffect::SlowFalling) {
            0.1
        } else if self.has(StatusEffect::Levitation) {
            -1.0
        } else {
            1.0
        }
    }

    pub fn strength_boost(&self) -> f32 {
        3.0 * self.get_level(StatusEffect::Strength)
    }

    pub fn weakness_penalty(&self) -> f32 {
        -4.0 * self.get_level(StatusEffect::Weakness)
    }

    pub fn damage_multiplier(&self) -> f32 {
        (1.0 - 0.2 * self.get_level(StatusEffect::Resistance)).max(0.0)
    }

    pub fn damage_multiplier_taken(&self) -> f32 {
        let resist = self.damage_multiplier();
        resist.max(0.0)
    }

    pub fn has_water_breathing(&self) -> bool {
        self.has(StatusEffect::WaterBreathing)
    }

    pub fn has_fire_resistance(&self) -> bool {
        self.has(StatusEffect::FireResistance)
    }

    pub fn has_night_vision(&self) -> bool {
        self.has(StatusEffect::NightVision)
    }

    pub fn has_invisibility(&self) -> bool {
        self.has(StatusEffect::Invisibility)
    }

    pub fn damage_per_second(&self) -> f32 {
        let mut dps = 0.0f32;
        for effect in &self.effects {
            let level = (effect.amplifier + 1) as f32;
            match effect.effect {
                StatusEffect::Poison | StatusEffect::Wither | StatusEffect::FatalPoison => {
                    dps += level * 1.0;
                }
                _ => {}
            }
        }
        dps
    }

    pub fn has_fatal_damage(&self) -> bool {
        self.has(StatusEffect::FatalPoison) || self.has(StatusEffect::Wither)
    }

    pub fn absorption_health(&self) -> f32 {
        4.0 * self.get_level(StatusEffect::Absorption)
    }
}
