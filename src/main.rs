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

    let mut ui = UI::new(width as f32, height as f32);
    let mut root = vec![];
    let play_button = ui.button("play");
    let stop_button = ui.button("stop");
    let mut controls = vec![play_button, stop_button];
    root.push(ui.row(controls));

    let mut load_sample_buttons = vec![];
    let mut tracks = vec![];
    let mut columns = vec![];
    for _ in 0..8 {
        let load_sample_button = ui.button("inst");
        load_sample_buttons.push(load_sample_button);
        let mut track = vec![];
        for _ in 0..8 {
            let textbox = ui.textbox();
            track.push(textbox);
        }
        let track_column = ui.column(track);
        tracks.push(track_column);
        columns.push(ui.column(vec![load_sample_button, track_column]));
    }
    let grid = ui.row(columns);
    root.push(grid);
    let column = ui.column(root);
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
    for (i, button) in load_sample_buttons.iter().enumerate() {
        ui.get_mut(*button).as_button().unwrap().on_press({
            let messages = messages.clone();
            move || {
                messages.borrow_mut().push_back(Message::LoadSample(i));
            }
        });
    }
    for (i, track) in tracks.iter().enumerate() {
        for j in 0..8 {
            let textbox = ui.get_mut(*track).as_column().unwrap().get_child(j);
            ui.get_mut(textbox).as_textbox().unwrap().on_change({
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
        }
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
