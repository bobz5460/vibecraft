mod status_effect;

pub use status_effect::{EffectManager, StatusEffect};

use crate::world::block::BlockId;
use crate::world::chunk_manager::ChunkManager;
use nalgebra::Point3;

pub const STANDING_EYE_HEIGHT: f32 = 1.6;
pub const SNEAK_EYE_HEIGHT: f32 = 1.27;
pub const SWIMMING_EYE_HEIGHT: f32 = 0.52;
pub const GRAVITY: f32 = -25.0;
pub const WATER_GRAVITY: f32 = -5.0;
pub const JUMP_SPEED: f32 = 8.0;
pub const WIDTH: f32 = 0.6;
pub const STANDING_HEIGHT: f32 = 1.8;
pub const SNEAK_HEIGHT: f32 = 1.5;
pub const SWIMMING_HEIGHT: f32 = 0.6;
pub const HALF_WIDTH: f32 = WIDTH / 2.0;
pub const MAX_HEALTH: f32 = 20.0;
pub const TERMINAL_VELOCITY: f32 = -78.4;
pub const WALK_SPEED: f32 = 4.317;
pub const SNEAK_SPEED: f32 = 1.295;
pub const SPRINT_MULT: f32 = 1.3;
pub const SWIM_SPEED: f32 = 3.0;
pub const SURFACE_SWIM_SPEED: f32 = 2.2;
pub const WATER_DRAG: f32 = 0.8;
pub const CLIMB_SPEED: f32 = 2.35;
pub const MAX_OXYGEN: f32 = 15.0;

pub const MAX_FOOD: f32 = 20.0;
pub const MAX_SATURATION: f32 = 20.0;
pub const EXHAUSTION_THRESHOLD: f32 = 4.0;
pub const STARVATION_DAMAGE_INTERVAL: f32 = 4.0;
pub const REGEN_COOLDOWN: f32 = 80.0;
pub const REGEN_SATURATION_COST: f32 = 1.5;
pub const REGEN_HUNGER_COST: f32 = 1.0;
pub const MIN_FOOD_FOR_REGEN: f32 = 18.0;

pub const ATTACK_COOLDOWN_DURATION: f32 = 0.5;
pub const CRITICAL_COOLDOWN_THRESHOLD: f32 = 0.848;

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vy: f32,
    pub on_ground: bool,
    pub health: f32,
    pub last_vy: f32,
    pub sneaking: bool,
    pub swimming: bool,
    pub oxygen: f32,
    pub hunger: f32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub attack_cooldown: f32,
    pub armor_points: f32,
    pub armor_toughness: f32,
    pub absorption_health: f32,
    pub damage_cooldown: f32,
    pub fall_flying: bool,
    pub effects: EffectManager,
}

impl Player {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Player {
            x,
            y,
            z,
            vy: 0.0,
            on_ground: false,
            health: MAX_HEALTH,
            last_vy: 0.0,
            sneaking: false,
            swimming: false,
            oxygen: MAX_OXYGEN,
            hunger: MAX_FOOD,
            saturation: MAX_SATURATION,
            exhaustion: 0.0,
            attack_cooldown: 1.0,
            armor_points: 0.0,
            armor_toughness: 0.0,
            absorption_health: 0.0,
            damage_cooldown: 0.0,
            fall_flying: false,
            effects: EffectManager::new(),
        }
    }

    pub fn current_height(&self) -> f32 {
        if self.swimming {
            SWIMMING_HEIGHT
        } else if self.sneaking {
            SNEAK_HEIGHT
        } else {
            STANDING_HEIGHT
        }
    }

    pub fn current_eye_height(&self) -> f32 {
        if self.swimming {
            SWIMMING_EYE_HEIGHT
        } else if self.sneaking {
            SNEAK_EYE_HEIGHT
        } else {
            STANDING_EYE_HEIGHT
        }
    }

    pub fn is_alive(&self) -> bool {
        self.health > 0.0
    }

    pub fn eye_position(&self) -> Point3<f32> {
        Point3::new(self.x, self.y + self.current_eye_height(), self.z)
    }

    pub fn apply_damage(&mut self, base_amount: f32, damage_mult: f32) -> f32 {
        if self.damage_cooldown > 0.0 {
            return 0.0;
        }
        let armor = self.armor_points.max(0.0);
        let toughness = self.armor_toughness.max(0.0);
        let effective_armor = armor - 4.0 * base_amount / (toughness + 8.0);
        let reduction_ratio = (effective_armor.max(0.0) / 25.0).min(0.8);
        let armor_reduction = 1.0 - reduction_ratio;
        let effect_reduction = self.effects.damage_multiplier_taken();
        let final_damage =
            (base_amount * damage_mult * armor_reduction * effect_reduction).max(0.0);
        if final_damage <= 0.0 {
            return 0.0;
        }
        let mut remaining = final_damage;
        if self.absorption_health > 0.0 {
            let absorbed = self.absorption_health.min(remaining);
            self.absorption_health -= absorbed;
            remaining -= absorbed;
        }
        if remaining > 0.0 {
            self.health = (self.health - remaining).max(0.0);
            self.damage_cooldown = 0.5;
        }
        final_damage
    }

    pub fn tick_hunger(&mut self, dt: f32, difficulty_mult: f32) {
        if self.hunger <= 0.0 && self.health > 0.0 && difficulty_mult > 0.0 {
            let dmg = (dt / STARVATION_DAMAGE_INTERVAL) * 1.0;
            self.apply_damage(dmg, 1.0);
        }
    }

    pub fn add_exhaustion(&mut self, amount: f32) {
        if self.hunger <= 0.0 {
            return;
        }
        self.exhaustion += amount;
        if self.exhaustion >= EXHAUSTION_THRESHOLD {
            self.exhaustion -= EXHAUSTION_THRESHOLD;
            if self.saturation > 0.0 {
                self.saturation = (self.saturation - 1.0).max(0.0);
            } else if self.hunger > 0.0 {
                self.hunger = (self.hunger - 1.0).max(0.0);
            }
        }
    }

    pub fn sprint_exhaustion(&mut self, dt: f32) {
        self.add_exhaustion(0.1 * dt.min(1.0));
    }

    pub fn jump_exhaustion(&mut self) {
        self.add_exhaustion(0.2);
    }

    pub fn attack_exhaustion(&mut self) {
        self.add_exhaustion(0.3);
    }

    pub fn damage_exhaustion(&mut self, damage: f32) {
        self.add_exhaustion(0.3 * damage.min(20.0));
    }

    pub fn tick_regen(&mut self, dt: f32, difficulty_regen_allowed: bool, is_peaceful: bool) {
        let mh = self.max_health();
        if self.health >= mh || self.health <= 0.0 {
            return;
        }
        if is_peaceful {
            self.health = (self.health + 1.0 * dt).min(mh);
            return;
        }
        if !difficulty_regen_allowed {
            return;
        }
        if self.hunger >= MIN_FOOD_FOR_REGEN && self.saturation > 0.0 {
            let regen_rate = 0.5;
            if self.saturation >= REGEN_SATURATION_COST {
                self.health = (self.health + regen_rate * dt).min(mh);
                self.saturation -= REGEN_SATURATION_COST * dt * regen_rate;
            }
        }
        if self.health < mh && self.health > 0.0 && self.hunger >= MIN_FOOD_FOR_REGEN && self.saturation <= 0.0 {
            let regen_rate = 0.25;
            let heal = regen_rate * dt;
            self.health = (self.health + heal).min(mh);
            self.hunger = (self.hunger - REGEN_HUNGER_COST * heal).max(0.0);
        }
    }

    pub fn tick_attack_cooldown(&mut self, dt: f32, weapon_speed: f32) {
        self.attack_cooldown = (self.attack_cooldown + weapon_speed * dt).min(1.0);
    }

    pub fn get_attack_cooldown(&self) -> f32 {
        self.attack_cooldown
    }

    pub fn can_critical_hit(&self) -> bool {
        self.attack_cooldown >= CRITICAL_COOLDOWN_THRESHOLD && !self.on_ground && self.vy < 0.0
    }

    pub fn reset_attack_cooldown(&mut self) {
        self.attack_cooldown = 0.0;
    }

    pub fn get_attack_damage(&self, base_damage: f32) -> f32 {
        let cooldown_mult = 0.2 + 0.8 * self.attack_cooldown;
        let str_boost = self.effects.strength_boost();
        let weak_penalty = self.effects.weakness_penalty();
        let raw = ((base_damage + str_boost + weak_penalty) * cooldown_mult).max(0.0);
        if self.can_critical_hit() {
            raw * 1.5
        } else {
            raw
        }
    }

    pub fn get_sprint_knockback(&self) -> f32 {
        if self.attack_cooldown >= CRITICAL_COOLDOWN_THRESHOLD {
            0.3
        } else {
            0.0
        }
    }

    pub fn apply_absorption(&mut self, amount: f32) {
        self.absorption_health = (self.absorption_health + amount).min(40.0);
    }

    pub fn max_health(&self) -> f32 {
        let boost = self.effects.get_level(StatusEffect::HealthBoost);
        MAX_HEALTH + 4.0 * boost
    }

    pub fn tick_effects(&mut self, dt: f32, damage_mult: f32) {
        if self.effects.has(StatusEffect::Regeneration) {
            let level = self.effects.get_level(StatusEffect::Regeneration);
            self.health = (self.health + (level as f32 + 1.0) * 0.4 * dt).min(self.max_health());
        }
        // Instant effects: process and remove immediately
        if self.effects.has(StatusEffect::InstantHealth) {
            let level = self.effects.get_level(StatusEffect::InstantHealth);
            let heal = 2.0 * level;
            self.health = (self.health + heal).min(self.max_health());
            self.effects.remove(StatusEffect::InstantHealth);
        }
        if self.effects.has(StatusEffect::InstantDamage) {
            let level = self.effects.get_level(StatusEffect::InstantDamage);
            let dmg = 3.0 * level * damage_mult;
            self.apply_damage(dmg, 1.0);
            self.effects.remove(StatusEffect::InstantDamage);
        }
        if self.effects.has(StatusEffect::SaturationEffect) {
            let level = self.effects.get_level(StatusEffect::SaturationEffect);
            let restore = 1.0 * level * dt;
            self.hunger = (self.hunger + restore).min(MAX_FOOD);
            self.saturation = (self.saturation + restore).min(MAX_SATURATION);
        }

        self.effects.tick(dt);
        let dps = self.effects.damage_per_second();
        if dps > 0.0 && self.health > 0.0 {
            if self.effects.has_fatal_damage() {
                // Wither and FatalPoison can kill
                self.health = (self.health - dps * dt * damage_mult).max(0.0);
            } else if self.health > 1.0 {
                // Regular Poison stops at 0.5 HP (1 half heart)
                self.health = (self.health - dps * dt * damage_mult).max(0.5);
            }
        }
        if self.effects.has(StatusEffect::Absorption) {
            let abs = self.effects.absorption_health();
            self.absorption_health = self.absorption_health.min(abs);
        } else {
            self.absorption_health = 0.0;
        }
    }

    pub fn get_speed_multiplier(&self) -> f32 {
        self.effects.speed_multiplier()
    }

    pub fn get_jump_multiplier(&self) -> f32 {
        self.effects.jump_multiplier()
    }

    pub fn get_gravity_multiplier(&self) -> f32 {
        self.effects.gravity_multiplier()
    }

    pub fn is_in_water(&self, cm: &ChunkManager) -> bool {
        let min_y = self.y.floor() as i32;
        let max_y = (self.y + self.current_height()).ceil() as i32;
        let min_x = (self.x - HALF_WIDTH).floor() as i32;
        let max_x = (self.x + HALF_WIDTH).ceil() as i32;
        let min_z = (self.z - HALF_WIDTH).floor() as i32;
        let max_z = (self.z + HALF_WIDTH).ceil() as i32;
        for by in min_y..=max_y {
            for bx in min_x..=max_x {
                for bz in min_z..=max_z {
                    if cm.get_block(bx, by, bz).id == BlockId::Water {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn is_fully_in_water(&self, cm: &ChunkManager) -> bool {
        let eye_y = (self.y + self.current_eye_height()).floor() as i32;
        let min_x = (self.x - HALF_WIDTH).floor() as i32;
        let max_x = (self.x + HALF_WIDTH).ceil() as i32;
        let min_z = (self.z - HALF_WIDTH).floor() as i32;
        let max_z = (self.z + HALF_WIDTH).ceil() as i32;
        for bx in min_x..=max_x {
            for bz in min_z..=max_z {
                if cm.get_block(bx, eye_y, bz).id == BlockId::Water {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_head_submerged(&self, cm: &ChunkManager) -> bool {
        let eye_y = (self.y + self.current_eye_height()).floor() as i32;
        let min_x = (self.x - HALF_WIDTH).floor() as i32;
        let max_x = (self.x + HALF_WIDTH).ceil() as i32;
        let min_z = (self.z - HALF_WIDTH).floor() as i32;
        let max_z = (self.z + HALF_WIDTH).ceil() as i32;
        (min_x..=max_x).any(|x| (min_z..=max_z).any(|z| cm.get_block(x, eye_y, z).id == BlockId::Water))
    }

    pub fn is_in_lava(&self, cm: &ChunkManager) -> bool {
        let min_y = self.y.floor() as i32;
        let max_y = (self.y + self.current_height()).ceil() as i32;
        let bx = self.x.floor() as i32;
        let bz = self.z.floor() as i32;
        for by in min_y..=max_y {
            for dx in [-1i32, 0, 1] {
                for dz in [-1i32, 0, 1] {
                    if cm.get_block(bx + dx, by, bz + dz).id == BlockId::Lava {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn is_suffocating(&self, cm: &ChunkManager) -> bool {
        let min_y = self.y.floor() as i32;
        let max_y = (self.y + self.current_height()).ceil() as i32;
        let min_x = (self.x - HALF_WIDTH).floor() as i32;
        let max_x = (self.x + HALF_WIDTH).floor() as i32;
        let min_z = (self.z - HALF_WIDTH).floor() as i32;
        let max_z = (self.z + HALF_WIDTH).floor() as i32;
        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let block = cm.get_block(bx, by, bz);
                    if block.id.is_solid()
                        && block.id != BlockId::Water
                        && block.id != BlockId::Lava
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn tick_damage(&mut self, dt: f32, cm: &ChunkManager, damage_mult: f32) {
        self.damage_cooldown = (self.damage_cooldown - dt).max(0.0);

        let height = self.current_height();
        let min_x = (self.x - HALF_WIDTH).floor() as i32;
        let max_x = (self.x + HALF_WIDTH).ceil() as i32;
        let min_y = self.y.floor() as i32;
        let max_y = (self.y + height).ceil() as i32;
        let min_z = (self.z - HALF_WIDTH).floor() as i32;
        let max_z = (self.z + HALF_WIDTH).ceil() as i32;

        let mut on_fire = false;
        let mut touching_cactus = false;
        let mut touching_berry_bush = false;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let block = cm.get_block(bx, by, bz);
                    if !on_fire && (block.id == BlockId::Fire || block.id == BlockId::SoulFire) {
                        on_fire = true;
                    }
                    if !touching_cactus && block.id == BlockId::Cactus {
                        touching_cactus = true;
                    }
                    if !touching_berry_bush && block.id == BlockId::SweetBerryBush {
                        touching_berry_bush = true;
                    }
                    if on_fire && touching_cactus && touching_berry_bush {
                        break;
                    }
                }
                if on_fire && touching_cactus && touching_berry_bush {
                    break;
                }
            }
            if on_fire && touching_cactus && touching_berry_bush {
                break;
            }
        }

        if self.effects.has_fire_resistance() {
            // skip fire and lava damage
        } else {
            if self.is_in_lava(cm) {
                self.apply_damage(4.0 * dt, damage_mult);
            }
            if on_fire {
                self.apply_damage(1.0 * dt, damage_mult);
            }
        }

        if touching_cactus {
            self.apply_damage(2.0 * dt, damage_mult);
        }

        if touching_berry_bush {
            self.apply_damage(1.0 * dt, damage_mult);
        }

        if self.is_suffocating(cm) {
            self.apply_damage(2.0 * dt, damage_mult);
        }

        if self.effects.has_water_breathing() {
            self.oxygen = (self.oxygen + dt * 3.0).min(MAX_OXYGEN);
        } else if self.is_head_submerged(cm) {
            self.oxygen = (self.oxygen - dt * 1.2).max(0.0);
            if self.oxygen <= 0.0 {
                self.apply_damage(2.0 * dt, damage_mult);
            }
        } else {
            self.oxygen = (self.oxygen + dt * 3.0).min(MAX_OXYGEN);
        }
    }

    pub fn collides(&self, x: f32, y: f32, z: f32, cm: &ChunkManager) -> bool {
        let height = self.current_height();
        let min_x = (x - HALF_WIDTH).floor() as i32;
        let max_x = (x + HALF_WIDTH).ceil() as i32;
        let min_y = y.floor() as i32;
        let max_y = (y + height).ceil() as i32;
        let min_z = (z - HALF_WIDTH).floor() as i32;
        let max_z = (z + HALF_WIDTH).ceil() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let block = cm.get_block(bx, by, bz);
                    if !block.id.is_solid() || block.id.is_crossed() || block.id.is_climbable() {
                        continue;
                    }
                    let px0 = x - HALF_WIDTH;
                    let px1 = x + HALF_WIDTH;
                    let py0 = y;
                    let py1 = y + height;
                    let pz0 = z - HALF_WIDTH;
                    let pz1 = z + HALF_WIDTH;

                    let bx0 = bx as f32;
                    let bx1 = bx as f32 + 1.0;
                    let (by0, by1) = if block.id.is_slab() {
                        if block.data & 1 == 0 { (by as f32, by as f32 + 0.5) } else { (by as f32 + 0.5, by as f32 + 1.0) }
                    } else {
                        (by as f32, by as f32 + 1.0)
                    };
                    let bz0 = bz as f32;
                    let bz1 = bz as f32 + 1.0;

                    if px0 < bx1 && px1 > bx0 && py0 < by1 && py1 > by0 && pz0 < bz1 && pz1 > bz0 {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn would_fall_off_at(&self, nx: f32, nz: f32, cm: &ChunkManager) -> bool {
        if !self.sneaking {
            return false;
        }
        let check_y = (self.y - 0.001).floor() as i32;
        let inset = HALF_WIDTH - 0.001;
        let corners = [
            (-inset, -inset),
            (-inset, inset),
            (inset, -inset),
            (inset, inset),
        ];
        for &(ox, oz) in &corners {
            let bx = (nx + ox).floor() as i32;
            let bz = (nz + oz).floor() as i32;
            let block = cm.get_block(bx, check_y, bz);
            if block.is_air() {
                return true;
            }
        }
        false
    }

    pub fn try_move_with_difficulty(
        &mut self,
        dx: f32,
        dy: f32,
        dz: f32,
        cm: &ChunkManager,
        damage_mult: f32,
    ) {
        let edge_check = self.sneaking;
        let nx = self.x + dx;
        if !edge_check || !self.would_fall_off_at(nx, self.z, cm) {
            if !self.collides(nx, self.y, self.z, cm) {
                self.x = nx;
            } else if self.collides(self.x, self.y - 0.05, self.z, cm) && !self.collides(nx, self.y + 0.6, self.z, cm) {
                self.y += 0.6;
                self.x = nx;
            }
        }
        let ny = self.y + dy;
        if !self.collides(self.x, ny, self.z, cm) {
            self.y = ny;
        } else if dy < 0.0 {
            self.on_ground = true;
            if !self.is_in_water(cm) && self.last_vy <= -10.0 {
                let fall_dist = (self.last_vy * self.last_vy) / (2.0 * -GRAVITY);
                let dmg = (fall_dist - 3.0).max(0.0).ceil();
                if dmg > 0.0 {
                    self.apply_damage(dmg, damage_mult);
                }
            }
            self.vy = 0.0;
            self.fall_flying = false;
        }
        let nz = self.z + dz;
        if !edge_check || !self.would_fall_off_at(self.x, nz, cm) {
            if !self.collides(self.x, self.y, nz, cm) {
                self.z = nz;
            } else if self.collides(self.x, self.y - 0.05, self.z, cm) && !self.collides(self.x, self.y + 0.6, nz, cm) {
                self.y += 0.6;
                self.z = nz;
            }
        }
        self.last_vy = self.vy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;
    use crate::world::chunk::Chunk;
    use std::sync::Arc;

    #[test]
    fn armor_and_absorption_reduce_damage_deterministically() {
        let mut player = Player::new(0.0, 64.0, 0.0);
        player.armor_points = 20.0;
        player.apply_absorption(2.0);
        assert!((player.apply_damage(10.0, 1.0) - 4.0).abs() < 0.01);
        assert_eq!(player.absorption_health, 0.0);
        assert!((player.health - 18.0).abs() < 0.01);
    }

    #[test]
    fn exhaustion_consumes_saturation_before_hunger() {
        let mut player = Player::new(0.0, 64.0, 0.0);
        player.saturation = 1.0;
        player.add_exhaustion(EXHAUSTION_THRESHOLD);
        assert_eq!(player.saturation, 0.0);
        assert_eq!(player.hunger, MAX_FOOD);
        player.add_exhaustion(EXHAUSTION_THRESHOLD);
        assert_eq!(player.hunger, MAX_FOOD - 1.0);
    }

    #[test]
    fn movement_stops_at_a_solid_block() {
        let mut manager = ChunkManager::new(11, 2);
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(1, 64, 0, Block::new(BlockId::Stone));
        manager.chunks.insert((0, 0), Arc::new(chunk));
        let mut player = Player::new(0.0, 64.0, 0.0);
        player.try_move_with_difficulty(1.0, 0.0, 0.0, &manager, 1.0);
        assert_eq!(player.x, 0.0);
    }
}
