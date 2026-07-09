use crate::world::chunk_manager::ChunkManager;
use nalgebra::Point3;

pub const EYE_HEIGHT: f32 = 1.6;
pub const GRAVITY: f32 = -25.0;
pub const JUMP_SPEED: f32 = 8.0;
pub const WIDTH: f32 = 0.6;
pub const HEIGHT: f32 = 1.8;
pub const HALF_WIDTH: f32 = WIDTH / 2.0;
pub const MAX_HEALTH: f32 = 20.0;

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vy: f32,
    pub on_ground: bool,
    pub health: f32,
    pub last_vy: f32,
    pub damage_flash: f32,
}

impl Player {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Player { x, y, z, vy: 0.0, on_ground: false, health: MAX_HEALTH, last_vy: 0.0, damage_flash: 0.0 }
    }

    pub fn eye_position(&self) -> Point3<f32> {
        Point3::new(self.x, self.y + EYE_HEIGHT, self.z)
    }

    pub fn take_damage(&mut self, amount: f32) {
        self.health = (self.health - amount).max(0.0);
        self.damage_flash = 0.3;
    }

    pub fn take_damage_with_mult(&mut self, base_amount: f32, mult: f32) {
        let final_damage = base_amount * mult;
        if final_damage > 0.0 {
            self.health = (self.health - final_damage).max(0.0);
            self.damage_flash = 0.3;
        }
    }

    pub fn is_alive(&self) -> bool {
        self.health > 0.0
    }

    pub fn collides(&self, x: f32, y: f32, z: f32, cm: &ChunkManager) -> bool {
        let min_x = (x - HALF_WIDTH).floor() as i32;
        let max_x = (x + HALF_WIDTH).ceil() as i32;
        let min_y = y.floor() as i32;
        let max_y = (y + HEIGHT).ceil() as i32;
        let min_z = (z - HALF_WIDTH).floor() as i32;
        let max_z = (z + HALF_WIDTH).ceil() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let block = cm.get_block(bx, by, bz);
                    if block.is_air() || block.id.is_transparent() {
                        continue;
                    }
                    let px0 = x - HALF_WIDTH;
                    let px1 = x + HALF_WIDTH;
                    let py0 = y;
                    let py1 = y + HEIGHT;
                    let pz0 = z - HALF_WIDTH;
                    let pz1 = z + HALF_WIDTH;

                    let bx0 = bx as f32;
                    let bx1 = bx as f32 + 1.0;
                    let by0 = by as f32;
                    let by1 = by as f32 + 1.0;
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

    pub fn try_move(&mut self, dx: f32, dy: f32, dz: f32, cm: &ChunkManager) {
        self.try_move_with_difficulty(dx, dy, dz, cm, 1.0);
    }

    pub fn try_move_with_difficulty(&mut self, dx: f32, dy: f32, dz: f32, cm: &ChunkManager, damage_mult: f32) {
        let nx = self.x + dx;
        if !self.collides(nx, self.y, self.z, cm) {
            self.x = nx;
        }
        let ny = self.y + dy;
        if !self.collides(self.x, ny, self.z, cm) {
            self.y = ny;
        } else if dy < 0.0 {
            self.on_ground = true;
            if self.last_vy < -10.0 {
                let fall_dist = (self.last_vy * self.last_vy) / (2.0 * -GRAVITY);
                let dmg = ((fall_dist - 3.0) / 3.0).max(0.0).ceil();
                if dmg > 0.0 { self.take_damage_with_mult(dmg, damage_mult); }
            }
            self.vy = 0.0;
        }
        let nz = self.z + dz;
        if !self.collides(self.x, self.y, nz, cm) {
            self.z = nz;
        }
        self.last_vy = self.vy;
    }
}
