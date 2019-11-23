extern crate glfw;
extern crate portaudio;

use glfw::{Action, Context, Key};

fn main() {
    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();

    let (mut window, events) = glfw.create_window(800, 600, "justitracker", glfw::WindowMode::Windowed).unwrap();

    window.set_key_polling(true);
    window.make_current();

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
