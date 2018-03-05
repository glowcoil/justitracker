mod ui;
mod render;
mod audio;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;
extern crate cpal;

use glium::glutin;

use render::*;
use ui::*;
use audio::*;

fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new()
        .with_dimensions(800, 600)
        .with_title("justitracker");
    let context = glutin::ContextBuilder::new();
    let display = glium::Display::new(window, context, &events_loop).unwrap();

    let (width, height, dpi_factor) = {
        let window = display.gl_window();
        let (width, height) = window.get_inner_size().unwrap();
        (width, height, window.hidpi_factor())
    };

    let mut renderer = Renderer::new(display, width, height, dpi_factor);

    let mut ui = UI::new(width as f32, height as f32);
    let button = ui.button("button");
    ui.make_root(button);

    let audio_send = start_audio_thread();

    events_loop.run_forever(|ev| {
        match ev {
            glutin::Event::WindowEvent { ref event, .. } => match *event {
                glutin::WindowEvent::Closed => return glutin::ControlFlow::Break,
                _ => {}
            },
            _ => {}
        };

        ui.handle_event(ev);

        while let Some(ui_event) = ui.get_event() {
            match ui_event {
                UIEvent::ButtonPress(id) => {
                    audio_send.send(1);
                    println!("{}", id);
                }
            }
        }

        renderer.render(ui.display());

        glutin::ControlFlow::Continue
    });
}
