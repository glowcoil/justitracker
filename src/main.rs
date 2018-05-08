mod ui;
mod render;
mod audio;

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
extern crate nfd;

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
    SetNote { track: usize, note: usize, factor: usize, power: i32 },
}

#[derive(Clone)]
pub struct Song {
    samples: Vec<Vec<f32>>,
    notes: Vec<Vec<Option<Vec<i32>>>>,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            samples: vec![vec![0.0; 1]; 8],
            notes: vec![vec![None; 8]; 8],
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
        let dpi_factor = window.hidpi_factor();
        (width, height, dpi_factor)
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
        let mut track: Vec<WidgetRef> = vec![load_sample_button];
        for j in 0..8 {
            let mut factors: Vec<WidgetRef> = vec![];
            for k in 0..4 {
                let factor = IntegerInput::new(0, font.clone());
                factor.borrow_mut().on_change({
                    let messages = messages.clone();
                    move |n| {
                        // let fraction: Vec<&str> = text.split("/").collect();
                        // if fraction.len() > 0 {
                        //     if let Ok(p) = fraction[0].parse::<f32>() {
                        //         let q = if fraction.len() == 2 {
                        //             if let Ok(q) = fraction[1].parse::<f32>() {
                        //                 q
                        //             } else {
                        //                 return;
                        //             }
                        //         } else {
                        //             1.0
                        //         };
                        //         messages.borrow_mut().push_back(Message::SetNote { track: i, note: j, pitch: p / q });
                        //     }
                        // }
                        messages.borrow_mut().push_back(Message::SetNote { track: i, note: j, factor: k, power: n });
                    }
                });
                factors.push(factor);
            }
            let note = Row::new(factors);
            // note.borrow_mut().get_style().min_width = Some(100.0);
            track.push(note);
        }
        columns.push(Column::new(track));
    }
    let grid = Row::new(columns);
    root.push(grid);

    ui.make_root(Column::new(root));

    let audio_send = start_audio_thread();

    renderer.render(ui.display());

    let mut cursor_hide = false;
    events_loop.run_forever(|ev| {
        let input_event = match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => {
                    return glutin::ControlFlow::Break;
                }
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    Some(InputEvent::CursorMoved { position: Point { x: x as f32, y: y as f32 } })
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
                                Some(InputEvent::MousePress { button: button })
                            }
                            glutin::ElementState::Released => {
                                Some(InputEvent::MouseRelease { button: button })
                            }
                        }
                    } else {
                        None
                    }
                }
                glutin::WindowEvent::KeyboardInput { device_id: _, input } => {
                    if let Some(keycode) = input.virtual_keycode {
                        let button = KeyboardButton::from_glutin(keycode);

                        match input.state {
                            glutin::ElementState::Pressed => {
                                Some(InputEvent::KeyPress { button: button })
                            }
                            glutin::ElementState::Released => {
                                Some(InputEvent::KeyRelease { button: button })
                            }
                        }
                    } else {
                        None
                    }
                }
                glutin::WindowEvent::ReceivedCharacter(c) => {
                    Some(InputEvent::TextInput { character: c })
                }
                _ => None,
            },
            _ => None,
        };

        if let Some(input_event) = input_event {
            let response = ui.handle_event(input_event);

            if response.mouse_cursor == MouseCursor::NoneCursor {
                if !cursor_hide {
                    renderer.get_display().gl_window().set_cursor_state(glutin::CursorState::Hide);
                    cursor_hide = true;
                }
            } else {
                if cursor_hide {
                    renderer.get_display().gl_window().set_cursor_state(glutin::CursorState::Normal);
                    cursor_hide = false;
                }
                renderer.get_display().gl_window().set_cursor(MouseCursor::to_glutin(response.mouse_cursor));
            }

            if let Some((x, y)) = response.set_mouse_position {
                renderer.get_display().gl_window().set_cursor_position(x as i32, y as i32);
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
                        if let Ok(result) = nfd::dialog().filter("wav").open() {
                            match result {
                                nfd::Response::Okay(path) => {
                                    let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
                                    song.samples[track] = samples;
                                    audio_send.send(AudioMessage::Song(song.clone())).unwrap();
                                }
                                _ => {}
                            }
                        }
                    }
                    Message::SetNote { track, note, factor, power } => {
                        if song.notes[track][note].is_none() {
                            song.notes[track][note] = Some(vec![0, 0, 0, 0]);
                        }
                        match song.notes[track][note].as_mut() {
                            Some(note) => { note[factor] = power; }
                            None => {}
                        }
                        audio_send.send(AudioMessage::Song(song.clone())).unwrap();
                    }
                }
            }

            renderer.render(ui.display());
        }

        glutin::ControlFlow::Continue
    });
}
