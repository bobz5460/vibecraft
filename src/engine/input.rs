use std::collections::HashSet;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct InputState {
    pub keys: HashSet<KeyCode>,
    pub mouse_buttons: HashSet<MouseButton>,
    pub mouse_delta: (f32, f32),
    pub mouse_grabbed: bool,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            keys: HashSet::new(),
            mouse_buttons: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            mouse_grabbed: true,
        }
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    pub fn is_mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons.contains(&button)
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
                    if pressed {
                        self.keys.insert(*keycode);
                    } else {
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
                    self.mouse_delta.0 += delta.0 as f32;
                    self.mouse_delta.1 += delta.1 as f32;
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
