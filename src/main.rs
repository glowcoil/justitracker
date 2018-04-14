mod ui;
mod render;
mod audio;
mod file;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate cocoa;
#[cfg(target_os = "macos")]
extern crate core_foundation;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;
extern crate cpal;
extern crate hound;
extern crate libc;

use glium::glutin;

use rusttype::FontCollection;

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

use render::*;
use ui::*;
use audio::*;

enum Message {
    Play,
    Stop,
    LoadSample(usize),
    SetNote { track: usize, note: usize, pitch: f32 },
}

#[derive(Clone)]
pub struct Song {
    samples: Vec<Vec<f32>>,
    notes: Vec<Vec<f32>>,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            samples: vec![vec![0.0; 1]; 8],
            notes: vec![vec![0.0; 8]; 8],
        }
    }
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
    let mut song: Song = Song::default();

    let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
    let font = Rc::new(collection.into_font().unwrap());

    let mut ui = UI::new(width as f32, height as f32);
    let mut root: Vec<WidgetRef> = vec![];
    let play_button = Button::with_text("play", font.clone());
    play_button.borrow_mut().on_press({
        let messages = messages.clone();
        move || {
            messages.borrow_mut().push_back(Message::Play);
        }
    });
    let stop_button = Button::with_text("stop", font.clone());
    stop_button.borrow_mut().on_press({
        let messages = messages.clone();
        move || {
            messages.borrow_mut().push_back(Message::Stop);
        }
    });
    let controls: Vec<WidgetRef> = vec![play_button, stop_button];
    root.push(Row::new(controls));

    let mut columns: Vec<WidgetRef> = vec![];
    for i in 0..8 {
        let load_sample_button = Button::with_text("inst", font.clone());
        load_sample_button.borrow_mut().on_press({
            let messages = messages.clone();
            move || {
                messages.borrow_mut().push_back(Message::LoadSample(i));
            }
        });
        let mut track: Vec<WidgetRef> = vec![];
        for j in 0..8 {
            let textbox = Textbox::new(font.clone());
            textbox.borrow_mut().on_change({
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
                            messages.borrow_mut().push_back(Message::SetNote { track: i, note: j, pitch: p / q });
                        }
                    }
                }
            });
            track.push(textbox);
        }
        let track_column = Column::new(track);
        columns.push(Column::new(vec![load_sample_button, track_column]));
    }
    let grid = Row::new(columns);
    root.push(grid);

    ui.make_root(Column::new(root));

    let audio_send = start_audio_thread();

    events_loop.run_forever(|ev| {
        match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => {
                    return glutin::ControlFlow::Break
                }
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    ui.handle_event(InputEvent::CursorMoved { position: Point { x: x as f32, y: y as f32 } });
                }
                glutin::WindowEvent::MouseInput { device_id: _, state, button, modifiers: _ } => {
                    let button = match button {
                        glutin::MouseButton::Left => Some(MouseButton::Left),
                        glutin::MouseButton::Middle => Some(MouseButton::Middle),
                        glutin::MouseButton::Right => Some(MouseButton::Right),
                        _ => None,
                    };

                    if let Some(button) = button {
                        match state {
                            glutin::ElementState::Pressed => {
                                ui.handle_event(InputEvent::MousePress { button: button });
                            }
                            glutin::ElementState::Released => {
                                ui.handle_event(InputEvent::MouseRelease { button: button });
                            }
                        }
                    }
                }
                glutin::WindowEvent::KeyboardInput { device_id: _, input } => {
                    if let Some(keycode) = input.virtual_keycode {
                        let button = KeyboardButton::from_glutin(keycode);

                        match input.state {
                            glutin::ElementState::Pressed => {
                                ui.handle_event(InputEvent::KeyPress { button: button });
                            }
                            glutin::ElementState::Released => {
                                ui.handle_event(InputEvent::KeyRelease { button: button });
                            }
                        }
                    }
                }
                glutin::WindowEvent::ReceivedCharacter(c) => {
                    ui.handle_event(InputEvent::TextInput { character: c });
                }
                _ => (),
            },
            _ => (),
        }

        while let Some(message) = messages.borrow_mut().pop_front() {
            match message {
                Message::Play => {
                    audio_send.send(AudioMessage::Play).unwrap();
                }
                Message::Stop => {
                    audio_send.send(AudioMessage::Stop).unwrap();
                }
                Message::LoadSample(track) => {
                    if let Some(path) = file::open_file() {
                        let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
                        song.samples[track] = samples;
                        audio_send.send(AudioMessage::Song(song.clone())).unwrap();
                    }
                }
                Message::SetNote { track, note, pitch } => {
                    song.notes[track][note] = pitch;
                    audio_send.send(AudioMessage::Song(song.clone())).unwrap();
                }
            }
        }

        renderer.render(ui.display());

        glutin::ControlFlow::Continue
    });
}
