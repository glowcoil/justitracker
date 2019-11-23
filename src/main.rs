mod audio;
mod window;

use audio::Audio;
use window::Window;

extern crate gl;
extern crate glfw;
extern crate gouache;
extern crate portaudio;

use glfw::{Action, Key, WindowEvent};
use gouache::{renderers::GlRenderer, Cache, Color, Font, Frame, Mat2x2, Vec2};

fn main() {
    let mut window = Window::new(800, 600, "justitracker");

    let mut cache = Cache::new();
    let mut renderer = GlRenderer::new();

    let font = Font::from_bytes(include_bytes!("../res/SourceSansPro-Regular.ttf")).unwrap();
    let text = font.layout("justitracker", 14.0);

    let mut audio = Audio::start().unwrap();

    let mut running = true;
    while running {
        let mut frame = Frame::new(&mut cache, &mut renderer, 800.0, 600.0);
        frame.clear(Color::rgba(0.1, 0.15, 0.2, 1.0));
        frame.draw_text(&font, &text, Vec2::new(0.0, 0.0), Mat2x2::id(), Color::rgba(1.0, 1.0, 1.0, 1.0));
        frame.finish();

        window.swap();

        window.poll(|event| {
            match event {
                WindowEvent::Close => { running = false; }
                WindowEvent::Key(Key::Escape, _, Action::Press, _) => { running = false; }
                _ => {}
            }
        });
    }
}
