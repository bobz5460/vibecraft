use nalgebra::{Matrix4, Perspective3, Point3, Vector3};

pub struct Camera {
    pub position: Point3<f32>,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub aspect: f32,
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
        Perspective3::new(self.aspect, self.fov, self.near, self.far).to_homogeneous()
    }

    pub fn vp_matrix(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn forward(&self) -> Vector3<f32> {
        Vector3::new(
            self.yaw.sin() * self.pitch.cos(),
            -self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn right(&self) -> Vector3<f32> {
        Vector3::new(
            self.yaw.cos(),
            0.0,
            -self.yaw.sin(),
        )
    }

    pub fn move_forward(&mut self, amount: f32) {
        let forward = self.forward();
        self.position += forward * amount;
    }

    pub fn move_right(&mut self, amount: f32) {
        let right = self.right();
        self.position += right * amount;
    }

    pub fn move_up(&mut self, amount: f32) {
        self.position.y += amount;
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx;
        self.pitch = (self.pitch + dy).clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
    }

    pub fn get_ray(&self) -> (Vector3<f32>, Vector3<f32>) {
        (self.position.coords, self.forward())
    }

    pub fn light_vp_matrix(&self, light_dir: &Vector3<f32>) -> Matrix4<f32> {
        let center = self.position;
        let light_pos = center - light_dir * 200.0;
        let up = Vector3::y();
        let view = Matrix4::look_at_rh(&light_pos, &center, &up);

        let size = 128.0;
        let near = 0.1;
        let far = 400.0;
        let proj = nalgebra::Orthographic3::new(-size, size, -size, size, near, far).to_homogeneous();
        proj * view
    }
}
