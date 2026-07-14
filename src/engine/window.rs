use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::Window;

pub struct WindowState {
    pub window: Arc<Window>,
    pub size: (u32, u32),
}

impl WindowState {
    // The winit `EventLoop::create_window` API is deprecated in 0.30+ in
    // favour of `ActiveEventLoop::create_window`.  Because we create the window
    // before entering the event loop, migration requires restructuring the
    // startup path.  Tracked in ISSUES.md.
    #[allow(deprecated)]
    pub fn new(event_loop: &EventLoop<()>) -> Result<Self, winit::error::OsError> {
        let size = LogicalSize::new(1280, 720);
        let window = event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_title("Vibecraft")
                    .with_inner_size(size)
                    .with_resizable(true),
            )
            ?;

        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        Ok(WindowState { window: Arc::new(window), size: (w, h) })
    }

    pub fn window(&self) -> &Window {
        self.window.as_ref()
    }

    pub fn resize(&mut self, new_size: (u32, u32)) {
        self.size = (new_size.0.max(1), new_size.1.max(1));
    }
}
