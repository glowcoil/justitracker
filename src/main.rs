mod render;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;

use glium::glutin;

use render::*;

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

    let mut text: String = "A japanese poem: 【『justitracker』 quick brown fox my jack daws of sphinx】".into();

    events_loop.run_forever(|ev| {
        match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => return glutin::ControlFlow::Break,
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => { text = format!("asdf {} {}", x, y); },
                _ => (),
            },
            _ => (),
        }

        let display_list = DisplayList {
            rects: vec![
                Rect { x: -0.5, y: 0.5, w: 0.2, h: 0.1, color: [0.2, 0.8, 0.3, 1.0] },
                Rect { x: -0.25, y: 0.25, w: 0.2, h: 0.1, color: [0.7, 0.2, 0.1, 1.0] },
            ],
            texts: vec![
                Text{ text: text.to_string(), x: 0.0, y: 0.0 },
                Text { text: "something".into(), x: 10.0, y: 20.0 }
            ],
        };
        renderer.render(display_list);

        glutin::ControlFlow::Continue
    });
}
