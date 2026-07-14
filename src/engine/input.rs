use rustc_hash::FxHashSet;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const MOUSE_DELTA_CAP: f32 = 1000.0;

pub struct InputState {
    pub keys: FxHashSet<KeyCode>,
    pub keys_just_pressed: FxHashSet<KeyCode>,
    pub keys_just_released: FxHashSet<KeyCode>,
    pub mouse_buttons: FxHashSet<MouseButton>,
    pub mouse_delta: (f32, f32),
    pub mouse_grabbed: bool,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            keys: FxHashSet::default(),
            keys_just_pressed: FxHashSet::default(),
            keys_just_released: FxHashSet::default(),
            mouse_buttons: FxHashSet::default(),
            mouse_delta: (0.0, 0.0),
            mouse_grabbed: true,
        }
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    pub fn is_key_just_pressed(&self, key: KeyCode) -> bool {
        self.keys_just_pressed.contains(&key)
    }

    pub fn is_key_just_released(&self, key: KeyCode) -> bool {
        self.keys_just_released.contains(&key)
    }

    pub fn is_mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons.contains(&button)
    }

    /// Call at end of frame to clear single-frame transition sets
    pub fn end_frame(&mut self) {
        self.keys_just_pressed.clear();
        self.keys_just_released.clear();
    }

    pub fn clear(&mut self) {
        self.keys.clear();
        self.keys_just_pressed.clear();
        self.keys_just_released.clear();
        self.mouse_buttons.clear();
        self.mouse_delta = (0.0, 0.0);
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        state,
                        physical_key,
                        ..
                    },
                ..
            } => {
                if let PhysicalKey::Code(keycode) = physical_key {
                    let pressed = *state == ElementState::Pressed;
                    let was_pressed = self.keys.contains(keycode);
                    if pressed {
                        if !was_pressed {
                            self.keys_just_pressed.insert(*keycode);
                        }
                        self.keys.insert(*keycode);
                    } else {
                        if was_pressed {
                            self.keys_just_released.insert(*keycode);
                        }
                        self.keys.remove(keycode);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                if pressed {
                    self.mouse_buttons.insert(*button);
                } else {
                    self.mouse_buttons.remove(button);
                }
            }
            _ => {}
        }
    }

    pub fn handle_device_event(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                if self.mouse_grabbed {
                    self.mouse_delta.0 = (self.mouse_delta.0 + delta.0 as f32).clamp(-MOUSE_DELTA_CAP, MOUSE_DELTA_CAP);
                    self.mouse_delta.1 = (self.mouse_delta.1 + delta.1 as f32).clamp(-MOUSE_DELTA_CAP, MOUSE_DELTA_CAP);
                }
            }
            _ => {}
        }
    }

    pub fn consume_mouse_delta(&mut self) -> (f32, f32) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        delta
    }
}
