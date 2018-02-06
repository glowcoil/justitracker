mod ui;
mod render;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;

use glium::glutin;

use render::*;
use ui::*;

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

    let mut ui = UI::new();
    let button = ui.add(Widget::Button { x: 10.0, y: 10.0, text: "button", state: ButtonState::Up });

    events_loop.run_forever(|ev| {
        ui.handle_event(ev);
        renderer.render(ui.display());

        glutin::ControlFlow::Continue
    });
}
