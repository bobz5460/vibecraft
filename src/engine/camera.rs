use nalgebra::{Matrix4, Perspective3, Point3, Vector3};
use std::cell::RefCell;

pub struct Camera {
    pub position: Point3<f32>,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub aspect: f32,
    cached_forward: RefCell<Option<Vector3<f32>>>,
}

impl Camera {
    pub fn new(position: Point3<f32>, aspect: f32) -> Self {
        Camera {
            position,
            yaw: 0.0,
            pitch: 0.0,
            fov: 70.0_f32.to_radians(),
            near: 0.1,
            far: 512.0,
            aspect,
            cached_forward: RefCell::new(None),
        }
    }

    pub fn view_matrix(&self) -> Matrix4<f32> {
        let forward = self.forward();
        let world_up = Vector3::y();
        let right = forward.cross(&world_up).normalize();
        let up = right.cross(&forward).normalize();

        let eye = self.position;
        let target = eye + forward;

        Matrix4::look_at_rh(&eye, &target, &up)
    }

    pub fn projection_matrix(&self) -> Matrix4<f32> {
        let opengl_projection =
            Perspective3::new(self.aspect, self.fov, self.near, self.far).to_homogeneous();
        // nalgebra uses OpenGL's -1..1 clip-space depth; wgpu requires 0..1.
        Matrix4::new(
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.5, 0.5,
            0.0, 0.0, 0.0, 1.0,
        ) * opengl_projection
    }

    pub fn vp_matrix(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn forward(&self) -> Vector3<f32> {
        if let Some(v) = *self.cached_forward.borrow() {
            return v;
        }
        let v = Vector3::new(
            self.yaw.sin() * self.pitch.cos(),
            -self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        *self.cached_forward.borrow_mut() = Some(v);
        v
    }

    pub fn right(&self) -> Vector3<f32> {
        // With yaw 0 looking toward +Z, -X is the screen-right direction.
        // This matches the basis used by `view_matrix`.
        Vector3::new(-self.yaw.cos(), 0.0, self.yaw.sin())
    }

    pub fn move_forward(&mut self, amount: f32) {
        let yaw_sin = self.yaw.sin();
        let yaw_cos = self.yaw.cos();
        self.position.x += yaw_sin * amount;
        self.position.z += yaw_cos * amount;
    }

    pub fn move_right(&mut self, amount: f32) {
        let right = self.right();
        self.position += right * amount;
    }

    pub fn move_up(&mut self, amount: f32) {
        self.position.y += amount;
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx).rem_euclid(2.0 * std::f32::consts::PI);
        self.pitch = (self.pitch + dy).clamp(-89.9_f32.to_radians(), 89.9_f32.to_radians());
        *self.cached_forward.borrow_mut() = None;
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    pub fn get_ray(&self) -> (Vector3<f32>, Vector3<f32>) {
        let fwd = self.forward();
        (self.position.coords + fwd * self.near, fwd)
    }

    /// Check if an AABB (axis-aligned bounding box) is inside the view frustum.
    /// Returns true if the box is partially or fully visible.
    /// Accepts a pre-computed VP matrix to avoid recomputing it per call.
    pub fn is_aabb_visible(
        &self,
        vp: &Matrix4<f32>,
        min_x: f32,
        min_y: f32,
        min_z: f32,
        max_x: f32,
        max_y: f32,
        max_z: f32,
    ) -> bool {
        let planes: [[f32; 4]; 6] = [
            [
                vp[(3, 0)] + vp[(0, 0)],
                vp[(3, 1)] + vp[(0, 1)],
                vp[(3, 2)] + vp[(0, 2)],
                vp[(3, 3)] + vp[(0, 3)],
            ],
            [
                vp[(3, 0)] - vp[(0, 0)],
                vp[(3, 1)] - vp[(0, 1)],
                vp[(3, 2)] - vp[(0, 2)],
                vp[(3, 3)] - vp[(0, 3)],
            ],
            [
                vp[(3, 0)] + vp[(1, 0)],
                vp[(3, 1)] + vp[(1, 1)],
                vp[(3, 2)] + vp[(1, 2)],
                vp[(3, 3)] + vp[(1, 3)],
            ],
            [
                vp[(3, 0)] - vp[(1, 0)],
                vp[(3, 1)] - vp[(1, 1)],
                vp[(3, 2)] - vp[(1, 2)],
                vp[(3, 3)] - vp[(1, 3)],
            ],
            [vp[(2, 0)], vp[(2, 1)], vp[(2, 2)], vp[(2, 3)]],
            [
                vp[(3, 0)] - vp[(2, 0)],
                vp[(3, 1)] - vp[(2, 1)],
                vp[(3, 2)] - vp[(2, 2)],
                vp[(3, 3)] - vp[(2, 3)],
            ],
        ];

        for &plane in &planes {
            let len = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
            if len < 1e-10 {
                continue;
            }
            let nx = plane[0] / len;
            let ny = plane[1] / len;
            let nz = plane[2] / len;
            let d = plane[3] / len;

            let px = if nx > 0.0 { max_x } else { min_x };
            let py = if ny > 0.0 { max_y } else { min_y };
            let pz = if nz > 0.0 { max_z } else { min_z };

            if nx * px + ny * py + nz * pz + d < 0.0 {
                return false;
            }
        }
        true
    }

    pub fn light_vp_matrix(&self, light_dir: &Vector3<f32>) -> Matrix4<f32> {
        let center = self.position;
        let light_pos = center + light_dir * 200.0;
        let up = Vector3::y();
        let view = Matrix4::look_at_rh(&light_pos, &center, &up);

        let size = 128.0;
        let near = 0.1;
        let far = 400.0;
        let proj = nalgebra::Orthographic3::new(-size, size, -size, size, near, far).to_homogeneous();
        proj * view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_basis_has_consistent_forward_and_screen_right() {
        let camera = Camera::new(Point3::origin(), 1.0);
        assert_eq!(camera.forward(), Vector3::new(0.0, 0.0, 1.0));
        assert_eq!(camera.right(), Vector3::new(-1.0, 0.0, 0.0));
    }
}
