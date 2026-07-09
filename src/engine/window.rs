use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::Window;

pub struct WindowState {
    pub window: Window,
    #[allow(dead_code)]
    pub size: (u32, u32),
}

impl WindowState {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        let size = LogicalSize::new(1280, 720);
        let window = event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_title("Vibecraft")
                    .with_inner_size(size)
                    .with_resizable(true),
            )
            .expect("Failed to create window");

        let inner_size = window.inner_size();
        WindowState {
            window,
            size: (inner_size.width, inner_size.height),
        }
    }
}
