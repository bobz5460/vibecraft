use crate::world::block::Block;
use crate::world::chunk_manager::ChunkManager;
use nalgebra::Vector3;

#[derive(Clone, Debug)]
pub struct RaycastHit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub normal: (i32, i32, i32),
    pub block: Block,
}

impl ChunkManager {
    pub fn raycast(
        &self,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        max_dist: f32,
    ) -> Option<RaycastHit> {
        if !max_dist.is_finite() || max_dist < 0.0 || direction.norm_squared() == 0.0 {
            return None;
        }
        let max_dist_sq = max_dist * max_dist;
        let dir = direction.normalize();
        let mut x = origin.x.floor() as i32;
        let mut y = origin.y.floor() as i32;
        let mut z = origin.z.floor() as i32;

        let step_x = if dir.x > 0.0 { 1 } else { -1 };
        let step_y = if dir.y > 0.0 { 1 } else { -1 };
        let step_z = if dir.z > 0.0 { 1 } else { -1 };

        let t_delta_x = if dir.x == 0.0 { f32::MAX } else { (1.0 / dir.x).abs() };
        let t_delta_y = if dir.y == 0.0 { f32::MAX } else { (1.0 / dir.y).abs() };
        let t_delta_z = if dir.z == 0.0 { f32::MAX } else { (1.0 / dir.z).abs() };

        let mut t_max_x = if dir.x > 0.0 {
            (x as f32 + 1.0 - origin.x) / dir.x
        } else if dir.x < 0.0 {
            (origin.x - x as f32) / -dir.x
        } else {
            f32::MAX
        };

        let mut t_max_y = if dir.y > 0.0 {
            (y as f32 + 1.0 - origin.y) / dir.y
        } else if dir.y < 0.0 {
            (origin.y - y as f32) / -dir.y
        } else {
            f32::MAX
        };

        let mut t_max_z = if dir.z > 0.0 {
            (z as f32 + 1.0 - origin.z) / dir.z
        } else if dir.z < 0.0 {
            (origin.z - z as f32) / -dir.z
        } else {
            f32::MAX
        };

        let mut last_normal: (i32, i32, i32) = (0, 0, 0);

        for _ in 0..(max_dist.ceil() as i32 * 3).max(1) {
            let block = self.get_block(x, y, z);
            if !block.is_air() {
                return Some(RaycastHit {
                    x,
                    y,
                    z,
                    normal: last_normal,
                    block,
                });
            }

            if t_max_x < t_max_y {
                if t_max_x < t_max_z {
                    x += step_x;
                    last_normal = (-step_x, 0, 0);
                    t_max_x += t_delta_x;
                } else {
                    z += step_z;
                    last_normal = (0, 0, -step_z);
                    t_max_z += t_delta_z;
                }
            } else {
                if t_max_y < t_max_z {
                    y += step_y;
                    last_normal = (0, -step_y, 0);
                    t_max_y += t_delta_y;
                } else {
                    z += step_z;
                    last_normal = (0, 0, -step_z);
                    t_max_z += t_delta_z;
                }
            }

            let dist_sq = (x as f32 - origin.x).powi(2)
                + (y as f32 - origin.y).powi(2)
                + (z as f32 - origin.z).powi(2);
            if dist_sq > max_dist_sq {
                return None;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::BlockId;
    use crate::world::chunk::Chunk;
    use std::sync::Arc;

    #[test]
    fn raycast_hits_expected_block_and_face() {
        let mut manager = ChunkManager::new(7, 2);
        let mut chunk = Chunk::new(0, 0);
        chunk.set_block(2, 10, 2, Block::new(BlockId::Stone));
        manager.chunks.insert((0, 0), Arc::new(chunk));
        let hit = manager
            .raycast(Vector3::new(0.5, 10.5, 2.5), Vector3::new(1.0, 0.0, 0.0), 5.0)
            .expect("ray must hit stone");
        assert_eq!((hit.x, hit.y, hit.z), (2, 10, 2));
        assert_eq!(hit.normal, (-1, 0, 0));
    }

    #[test]
    fn zero_direction_returns_no_hit() {
        let manager = ChunkManager::new(7, 2);
        assert!(manager.raycast(Vector3::zeros(), Vector3::zeros(), 5.0).is_none());
    }
}
