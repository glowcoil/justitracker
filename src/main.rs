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

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

use render::*;
use ui::*;
use audio::*;

enum Message {
    Play,
    Stop,
    SetNote { note: usize, pitch: f32 },
}

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

    let messages: Rc<RefCell<VecDeque<Message>>> = Rc::new(RefCell::new(VecDeque::new()));
    let mut song: [f32; 8] = [0.0; 8];

    let mut ui = UI::new(width as f32, height as f32);
    let play_button = ui.button("play");
    let stop_button = ui.button("stop");
    let mut children = vec![play_button, stop_button];
    let mut boxes = vec![];
    for i in 0..8 {
        let textbox = ui.textbox();
        children.push(textbox);
        boxes.push(textbox);
    }
    let column = ui.column(children);
    ui.make_root(column);

    ui.get_mut(play_button).as_button().unwrap().on_press({
        let messages = messages.clone();
        move || {
            messages.borrow_mut().push_back(Message::Play);
        }
    });
    ui.get_mut(stop_button).as_button().unwrap().on_press({
        let messages = messages.clone();
        move || {
            messages.borrow_mut().push_back(Message::Stop);
        }
    });
    for (i, textbox) in boxes.iter().enumerate() {
        ui.get_mut(*textbox).as_textbox().unwrap().on_change({
            let messages = messages.clone();
            move |text| {
                let fraction: Vec<&str> = text.split("/").collect();
                if fraction.len() > 0 {
                    if let Ok(p) = fraction[0].parse::<f32>() {
                        let q = if fraction.len() == 2 {
                            if let Ok(q) = fraction[1].parse::<f32>() {
                                q
                            } else {
                                return;
                            }
                        } else {
                            1.0
                        };
                        messages.borrow_mut().push_back(Message::SetNote { note: i, pitch: p / q });
                    }
                }
            }
        });
    }

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

        while let Some(message) = messages.borrow_mut().pop_front() {
            match message {
                Message::Play => {
                    audio_send.send(AudioMessage::Play).unwrap();
                }
                Message::Stop => {
                    audio_send.send(AudioMessage::Stop).unwrap();
                }
                Message::SetNote { note, pitch } => {
                    song[note] = pitch;
                    audio_send.send(AudioMessage::Song(song)).unwrap();
                }
            }
        }

        renderer.render(ui.display());

        glutin::ControlFlow::Continue
    });
}
