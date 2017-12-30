mod text;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;

use glium::{glutin, Surface};

fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new()
        .with_dimensions(800, 600)
        .with_title("justitracker");
    let context = glutin::ContextBuilder::new();
    let display = glium::Display::new(window, context, &events_loop).unwrap();

    let mut text: String = "A japanese poem: 【『justitracker』 quick brown fox my jack daws of sphinx】"
            .into();

    let (width, dpi_factor) = {
        let window = display.gl_window();
        (window.get_inner_size().unwrap().0, window.hidpi_factor())
    };

    let mut text_renderer = text::TextRenderer::new(&display, include_bytes!("../EPKGOBLD.TTF"), width, dpi_factor);

    #[derive(Copy, Clone)]
    struct Vertex {
        position: [f32; 2],
    }
    implement_vertex!(Vertex, position);

    let vertex_buffer = glium::VertexBuffer::new(&display, &[
        Vertex { position: [-0.5, -0.5] },
        Vertex { position: [0.5, -0.5] },
        Vertex { position: [0.5, 0.5] },
        Vertex { position: [-0.5, 0.5] },
    ]).unwrap();

    let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
    let index_buffer = glium::IndexBuffer::new(&display, glium::index::PrimitiveType::TrianglesList, &indices).unwrap();

    let vertex_shader_src = r#"
        #version 140

        in vec2 position;

        void main() {
            gl_Position = vec4(position, 0.0, 1.0);
        }
    "#;

    let fragment_shader_src = r#"
        #version 140

        out vec4 color;

        void main() {
            color = vec4(1.0, 0.0, 0.0, 1.0);
        }
    "#;


    let program = glium::Program::from_source(&display, vertex_shader_src, fragment_shader_src, None).unwrap();

    events_loop.run_forever(|ev| {
        match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => return glutin::ControlFlow::Break,
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => { text = format!("asdf {} {}", x, y); },
                _ => (),
            },
            _ => (),
        }

        let mut target = display.draw();
        target.clear_color(0.0, 0.03, 0.1, 1.0);

        target.draw(
            &vertex_buffer,
            &index_buffer,
            &program,
            &glium::uniforms::EmptyUniforms,
            &Default::default()).unwrap();

        text_renderer.draw(&mut target, width, &text[..]);

        target.finish().unwrap();

        glutin::ControlFlow::Continue
    });
}
