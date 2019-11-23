extern crate gl;
extern crate glfw;
extern crate gouache;
extern crate portaudio;

use glfw::{Action, Context, Key};
use gouache::{renderers::GlRenderer, Cache, Color, Font, Frame, Mat2x2, Vec2};

fn main() {
    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
    glfw.window_hint(glfw::WindowHint::ContextVersion(3, 3));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
    glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
    let (mut window, events) = glfw.create_window(800, 600, "justitracker", glfw::WindowMode::Windowed).unwrap();

    window.set_key_polling(true);
    window.make_current();

    gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

    let mut cache = Cache::new();
    let mut renderer = GlRenderer::new();

    let font = Font::from_bytes(include_bytes!("../res/SourceSansPro-Regular.ttf")).unwrap();
    let text = font.layout("justitracker", 14.0);

    audio().unwrap();

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    window.set_should_close(true)
                }
                _ => {}
            }
        }

        let mut frame = Frame::new(&mut cache, &mut renderer, 800.0, 600.0);

        frame.clear(Color::rgba(0.1, 0.15, 0.2, 1.0));
        frame.draw_text(&font, &text, Vec2::new(0.0, 0.0), Mat2x2::id(), Color::rgba(1.0, 1.0, 1.0, 1.0));
        frame.finish();

        window.swap_buffers();
    }
}

use portaudio as pa;

const SAMPLE_RATE: f64 = 44_100.0;
const FRAMES: u32 = 256;
const CHANNELS: i32 = 2;

fn audio() -> Result<(), pa::Error> {
    let pa = pa::PortAudio::new()?;
    let settings = pa.default_output_stream_settings(CHANNELS, SAMPLE_RATE, FRAMES)?;
    let mut stream = pa.open_non_blocking_stream(settings, move |args| {
        assert!(args.frames == FRAMES as usize);

        for sample in args.buffer.iter_mut() {
            *sample = 0.0;
        }

        pa::Continue
    })?;

    stream.start()?;
    stream.stop()?;

    Ok(())
}
