use glfw::Context;

const FRAME: std::time::Duration = std::time::Duration::from_micros(1_000_000 / 60);

pub struct Window {
    glfw: glfw::Glfw,
    window: glfw::Window,
    last_frame: std::time::Instant,
}

impl Window {
    pub fn new(width: u32, height: u32, title: &str) -> Window {
        let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
        glfw.window_hint(glfw::WindowHint::ContextVersion(3, 3));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
        glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        let (mut window, _) = glfw.create_window(800, 600, "justitracker", glfw::WindowMode::Windowed).unwrap();

        window.set_key_polling(true);
        window.make_current();

        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

        Window { glfw, window, last_frame: std::time::Instant::now() }
    }

    pub fn poll(&mut self, mut f: impl FnMut(glfw::WindowEvent))  {
        self.glfw.poll_events_unbuffered(|_, (_, event)| {
            f(event);
            None
        });
    }

    pub fn swap(&mut self) {
        self.window.swap_buffers();

        let elapsed = self.last_frame.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
        self.last_frame = std::time::Instant::now();
    }

    pub fn should_close(&self) -> bool {
        self.window.should_close()
    }
}
