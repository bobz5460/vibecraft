//! Minimal simulation-only entities and projectiles.
//!
//! Hitboxes are local to `Transform::position`; rendering, persistence, and
//! higher-level combat rules intentionally remain outside this substrate.

use std::collections::BTreeMap;

use nalgebra::Vector3;

use crate::world::chunk_manager::ChunkManager;

pub const FIXED_TICK_SECONDS: f32 = 1.0 / 20.0;
const GRAVITY: f32 = 9.81;
const MAX_MOTION_STEP: f32 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub position: Vector3<f32>,
}

impl Transform {
    pub const fn new(position: Vector3<f32>) -> Self {
        Self { position }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
}

impl Aabb {
    pub const fn new(min: Vector3<f32>, max: Vector3<f32>) -> Self {
        Self { min, max }
    }

    pub fn translated(self, offset: Vector3<f32>) -> Self {
        Self::new(self.min + offset, self.max + offset)
    }

    pub fn intersects(self, other: Self) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    TrainingDummy,
}

impl EntityKind {
    fn default_health(self) -> f32 {
        match self {
            Self::TrainingDummy => 20.0,
        }
    }

    fn default_aabb(self) -> Aabb {
        match self {
            Self::TrainingDummy => Aabb::new(
                Vector3::new(-0.3, 0.0, -0.3),
                Vector3::new(0.3, 1.8, 0.3),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub transform: Transform,
    pub velocity: Vector3<f32>,
    pub health: f32,
    /// Local hitbox translated by `transform.position` for world queries.
    pub aabb: Aabb,
}

impl Entity {
    fn new(id: EntityId, kind: EntityKind, transform: Transform) -> Self {
        Self {
            id,
            kind,
            transform,
            velocity: Vector3::zeros(),
            health: kind.default_health(),
            aabb: kind.default_aabb(),
        }
    }

    pub fn world_aabb(&self) -> Aabb {
        self.aabb.translated(self.transform.position)
    }

    pub fn is_alive(&self) -> bool {
        self.health > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRaycastHit {
    pub entity: EntityId,
    pub distance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrowProjectile {
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub damage: f32,
    pub gravity: f32,
    pub owner: Option<EntityId>,
    pub age_ticks: u32,
    pub max_age_ticks: u32,
}

impl ArrowProjectile {
    pub fn new(position: Vector3<f32>, velocity: Vector3<f32>, damage: f32) -> Self {
        Self {
            position,
            velocity,
            damage,
            gravity: GRAVITY,
            owner: None,
            age_ticks: 0,
            max_age_ticks: 1_200,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Projectile {
    Arrow(ArrowProjectile),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileHit {
    Block { x: i32, y: i32, z: i32 },
    Entity(EntityId),
}

pub struct EntityStore {
    entities: BTreeMap<EntityId, Entity>,
    projectiles: Vec<Projectile>,
    next_id: u64,
}

impl Default for EntityStore {
    fn default() -> Self {
        Self {
            entities: BTreeMap::new(),
            projectiles: Vec::new(),
            next_id: 1,
        }
    }
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self, kind: EntityKind, transform: Transform) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect("entity ID space exhausted");
        self.entities.insert(id, Entity::new(id, kind, transform));
        id
    }

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    pub fn despawn(&mut self, id: EntityId) -> Option<Entity> {
        self.entities.remove(&id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }

    pub fn spawn_projectile(&mut self, projectile: Projectile) {
        self.projectiles.push(projectile);
    }

    pub fn spawn_arrow(&mut self, arrow: ArrowProjectile) {
        self.spawn_projectile(Projectile::Arrow(arrow));
    }

    pub fn query_aabb(&self, area: Aabb) -> Vec<EntityId> {
        self.entities
            .values()
            .filter(|entity| entity.is_alive() && entity.world_aabb().intersects(area))
            .map(|entity| entity.id)
            .collect()
    }

    pub fn raycast(
        &self,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        max_distance: f32,
    ) -> Option<EntityRaycastHit> {
        self.raycast_except(origin, direction, max_distance, None)
    }

    /// Applies direct melee damage and an additive knockback velocity.
    pub fn melee_damage(&mut self, target: EntityId, damage: f32, knockback: Vector3<f32>) -> bool {
        if !damage.is_finite() || damage <= 0.0 || !knockback.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(entity) = self.entities.get_mut(&target) else {
            return false;
        };
        if !entity.is_alive() {
            return false;
        }
        entity.health = (entity.health - damage).max(0.0);
        entity.velocity += knockback;
        true
    }

    /// Advances entity physics and projectile simulation by one 20 TPS tick.
    pub fn tick(&mut self, chunks: &ChunkManager) -> Vec<ProjectileHit> {
        for entity in self.entities.values_mut().filter(|entity| entity.is_alive()) {
            tick_entity(entity, chunks);
        }

        let mut events = Vec::new();
        let mut remaining = Vec::with_capacity(self.projectiles.len());
        for projectile in std::mem::take(&mut self.projectiles) {
            match projectile {
                Projectile::Arrow(mut arrow) => {
                    if let Some(hit) = self.tick_arrow(&mut arrow, chunks) {
                        events.push(hit);
                    } else if arrow.age_ticks < arrow.max_age_ticks {
                        remaining.push(Projectile::Arrow(arrow));
                    }
                }
            }
        }
        self.projectiles = remaining;
        events
    }

    fn raycast_except(
        &self,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        max_distance: f32,
        ignored: Option<EntityId>,
    ) -> Option<EntityRaycastHit> {
        if !max_distance.is_finite() || max_distance < 0.0 || direction.norm_squared() == 0.0 {
            return None;
        }
        let direction = direction.normalize();
        self.entities
            .values()
            .filter(|entity| Some(entity.id) != ignored && entity.is_alive())
            .filter_map(|entity| {
                ray_aabb_distance(origin, direction, entity.world_aabb())
                    .filter(|&distance| distance <= max_distance)
                    .map(|distance| EntityRaycastHit { entity: entity.id, distance })
            })
            .min_by(|left, right| left.distance.total_cmp(&right.distance))
    }

    fn tick_arrow(&mut self, arrow: &mut ArrowProjectile, chunks: &ChunkManager) -> Option<ProjectileHit> {
        let displacement = arrow.velocity * FIXED_TICK_SECONDS;
        let distance = displacement.norm();
        if distance > 0.0 {
            let block_hit = chunks.raycast(arrow.position, displacement, distance);
            let entity_hit = self.raycast_except(arrow.position, displacement, distance, arrow.owner);
            if let Some(entity_hit) = entity_hit.filter(|hit| {
                block_hit.as_ref().is_none_or(|block| {
                    hit.distance <= block_distance(arrow.position, displacement, block)
                })
            }) {
                self.melee_damage(entity_hit.entity, arrow.damage, Vector3::zeros());
                return Some(ProjectileHit::Entity(entity_hit.entity));
            }
            if let Some(block) = block_hit {
                return Some(ProjectileHit::Block { x: block.x, y: block.y, z: block.z });
            }
            arrow.position += displacement;
        }
        arrow.velocity.y -= arrow.gravity * FIXED_TICK_SECONDS;
        arrow.age_ticks += 1;
        None
    }
}

fn tick_entity(entity: &mut Entity, chunks: &ChunkManager) {
    entity.velocity.y -= GRAVITY * FIXED_TICK_SECONDS;
    move_entity(entity, chunks, entity.velocity * FIXED_TICK_SECONDS);
}

fn move_entity(entity: &mut Entity, chunks: &ChunkManager, delta: Vector3<f32>) {
    let steps = (delta.abs().max() / MAX_MOTION_STEP).ceil().max(1.0) as u32;
    let step = delta / steps as f32;
    for _ in 0..steps {
        for axis in 0..3 {
            let mut next = entity.transform.position;
            next[axis] += step[axis];
            if collides_blocks(entity.aabb.translated(next), chunks) {
                entity.velocity[axis] = 0.0;
            } else {
                entity.transform.position[axis] = next[axis];
            }
        }
    }
}

fn collides_blocks(aabb: Aabb, chunks: &ChunkManager) -> bool {
    let min_x = aabb.min.x.floor() as i32;
    let max_x = aabb.max.x.ceil() as i32 - 1;
    let min_y = aabb.min.y.floor() as i32;
    let max_y = aabb.max.y.ceil() as i32 - 1;
    let min_z = aabb.min.z.floor() as i32;
    let max_z = aabb.max.z.ceil() as i32 - 1;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                let block = chunks.get_block(x, y, z);
                if !block.id.is_solid() || block.id.is_crossed() || block.id.is_climbable() {
                    continue;
                }
                let top = if block.id.is_slab() && block.data & 1 == 0 {
                    y as f32 + 0.5
                } else {
                    y as f32 + 1.0
                };
                let block_aabb = Aabb::new(
                    Vector3::new(x as f32, y as f32, z as f32),
                    Vector3::new(x as f32 + 1.0, top, z as f32 + 1.0),
                );
                if aabb.intersects(block_aabb) {
                    return true;
                }
            }
        }
    }
    false
}

fn ray_aabb_distance(origin: Vector3<f32>, direction: Vector3<f32>, aabb: Aabb) -> Option<f32> {
    let mut near = 0.0f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < f32::EPSILON {
            if origin[axis] < aabb.min[axis] || origin[axis] > aabb.max[axis] {
                return None;
            }
            continue;
        }
        let inverse = direction[axis].recip();
        let mut t0 = (aabb.min[axis] - origin[axis]) * inverse;
        let mut t1 = (aabb.max[axis] - origin[axis]) * inverse;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        near = near.max(t0);
        far = far.min(t1);
        if far < near {
            return None;
        }
    }
    Some(near)
}

fn block_distance(
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    block: &crate::world::raycast::RaycastHit,
) -> f32 {
    let block_aabb = Aabb::new(
        Vector3::new(block.x as f32, block.y as f32, block.z as f32),
        Vector3::new(block.x as f32 + 1.0, block.y as f32 + 1.0, block.z as f32 + 1.0),
    );
    ray_aabb_distance(origin, direction.normalize(), block_aabb).unwrap_or(f32::INFINITY)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::world::block::{Block, BlockId};
    use crate::world::chunk::Chunk;

    #[test]
    fn spatial_query_and_raycast_return_nearest_live_entity() {
        let mut store = EntityStore::new();
        let near = store.spawn(EntityKind::TrainingDummy, Transform::new(Vector3::new(2.0, 4.0, 0.0)));
        let far = store.spawn(EntityKind::TrainingDummy, Transform::new(Vector3::new(5.0, 4.0, 0.0)));

        assert_eq!(near.get(), 1);
        assert_eq!(store.query_aabb(Aabb::new(Vector3::new(1.5, 3.0, -1.0), Vector3::new(2.5, 6.0, 1.0))), vec![near]);
        assert_eq!(store.raycast(Vector3::new(0.0, 4.8, 0.0), Vector3::x(), 10.0).unwrap().entity, near);
        assert_ne!(near, far);
    }

    #[test]
    fn melee_damage_applies_knockback_and_does_not_revive_dead_entities() {
        let mut store = EntityStore::new();
        let target = store.spawn(EntityKind::TrainingDummy, Transform::default());

        assert!(store.melee_damage(target, 7.0, Vector3::new(1.0, 0.5, 0.0)));
        let entity = store.get(target).unwrap();
        assert_eq!(entity.health, 13.0);
        assert_eq!(entity.velocity, Vector3::new(1.0, 0.5, 0.0));
        assert!(store.melee_damage(target, 20.0, Vector3::zeros()));
        assert!(!store.melee_damage(target, 1.0, Vector3::zeros()));
    }

    #[test]
    fn arrow_hits_entity_before_block_and_applies_damage() {
        let mut chunks = ChunkManager::new(7, 1, crate::world::coordinates::WorldCoordinateProfile::LegacyLocal, crate::world::generation::WorldGenerationProfile::legacy());
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(4, 4, 0, Block::new(BlockId::Stone));
        chunks.chunks.insert((0, 0), Arc::new(chunk));
        let mut store = EntityStore::new();
        let target = store.spawn(EntityKind::TrainingDummy, Transform::new(Vector3::new(2.0, 4.0, 0.0)));
        store.spawn_arrow(ArrowProjectile::new(Vector3::new(0.0, 4.8, 0.0), Vector3::new(60.0, 0.0, 0.0), 5.0));

        assert_eq!(store.tick(&chunks), vec![ProjectileHit::Entity(target)]);
        assert_eq!(store.get(target).unwrap().health, 15.0);
        assert!(store.projectiles().is_empty());
    }

    #[test]
    fn arrow_reports_block_hit_and_entities_stop_at_solid_blocks() {
        let mut chunks = ChunkManager::new(7, 1, crate::world::coordinates::WorldCoordinateProfile::LegacyLocal, crate::world::generation::WorldGenerationProfile::legacy());
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(1, 0, 0, Block::new(BlockId::Stone));
        chunk.set_block(2, 4, 0, Block::new(BlockId::Stone));
        chunks.chunks.insert((0, 0), Arc::new(chunk));
        let mut store = EntityStore::new();
        let target = store.spawn(EntityKind::TrainingDummy, Transform::new(Vector3::new(0.5, 1.0, 0.5)));
        store.spawn_arrow(ArrowProjectile::new(Vector3::new(0.0, 4.5, 0.5), Vector3::new(60.0, 0.0, 0.0), 2.0));

        assert_eq!(store.tick(&chunks), vec![ProjectileHit::Block { x: 2, y: 4, z: 0 }]);
        for _ in 0..40 {
            store.tick(&chunks);
        }
        assert!(store.get(target).unwrap().transform.position.x < 0.7);
    }
}
