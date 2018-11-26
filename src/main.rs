mod ui;
mod render;
mod audio;

#[macro_use]
extern crate glium;
extern crate rusttype;
extern crate arrayvec;
extern crate unicode_normalization;
extern crate cpal;
extern crate hound;
extern crate libc;
extern crate nfd;
extern crate anymap;
extern crate unsafe_any;
extern crate slab;
extern crate priority_queue;

use glium::glutin;

use rusttype::{FontCollection, Font, Scale};

use std::sync::mpsc;
use std::rc::Rc;

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
    bpm: u32,
    ptn_len: usize,
    samples: Vec<Vec<f32>>,
    notes: Vec<Vec<Note>>,
}

#[derive(Clone)]
pub enum Note {
    On(Vec<i32>),
    Off,
    None,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            bpm: 120,
            ptn_len: 8,
            samples: vec![vec![0.0; 1]; 8],
            notes: vec![vec![Note::None; 8]; 8],
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

    let mut ui = UI::new(width as f32, height as f32);

    struct App {
        song: Song,
        audio_send: mpsc::Sender<AudioMessage>,
        cursor: (usize, usize),
        font: Rc<Font<'static>>,
    }
    impl App {
        fn update(&mut self) {
            self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
        }

        fn set_ptn_len(&mut self, len: usize) {
            if len < self.song.ptn_len {
                for track in 0..self.song.notes.len() {
                    self.song.notes[track].truncate(len);
                }
            } else if len > self.song.ptn_len {
                for track in 0..self.song.notes.len() {
                    self.song.notes[track].resize(len, Note::None);
                }
            }
            self.song.ptn_len = len;
        }

        fn add_track(&mut self) {
            self.song.samples.push(vec![0.0; 1]);
            let mut track = Vec::with_capacity(self.song.ptn_len);
            track.resize(self.song.ptn_len, Note::None);
            self.song.notes.push(track);
        }

        fn delete_track(&mut self, track: usize) {
            self.song.samples.remove(track);
            self.song.notes.remove(track);
        }
    }
    impl Component for App {
        fn install(&self, context: &mut InstallContext<App>, children: &[Child]) {
            let style = TextStyle { font: self.font.clone(), scale: Scale::uniform(19.0) };

            let mut root = context.root().get_or_place(|| Col::new(5.0));

            {
                let mut controls = root.child().get_or_place(|| Row::new(5.0));
                {
                    let mut play = controls.child().get_or_place(|| Button::new());
                    play.listen(|ctx, ClickEvent| { ctx.get_mut().audio_send.send(AudioMessage::Play).unwrap() });
                    play.child().get_or_place(|| Text::new("play".to_string(), style.clone()));
                }
                {
                    let mut stop = controls.child().get_or_place(|| Button::new());
                    stop.listen(|ctx, ClickEvent| { ctx.get_mut().audio_send.send(AudioMessage::Stop).unwrap() });
                    stop.child().get_or_place(|| Text::new("stop".to_string(), style.clone()));
                }
                controls.child().get_or_place(|| Text::new("bpm:".to_string(), style.clone()));
                {
                    let mut bpm = controls.child().get_or_place(|| IntegerInput::new(self.song.bpm as i32, style.clone()));
                    bpm.get_mut().value(self.song.bpm as i32);
                    bpm.listen(|ctx, value: i32| {
                        ctx.get_mut().song.bpm = value as u32;
                        ctx.get_mut().update();
                    });
                }
                controls.child().get_or_place(|| Text::new("len:".to_string(), style.clone()));
                {
                    let mut ptn_len = controls.child().get_or_place(|| IntegerInput::new(self.song.ptn_len as i32, style.clone()));
                    ptn_len.get_mut().value(self.song.ptn_len as i32);
                    ptn_len.listen(|ctx, value: i32| {
                        let value = value.max(1) as usize;
                        ctx.get_mut().set_ptn_len(value);
                        ctx.get_mut().update();
                    });
                }
                {
                    let mut add = controls.child().get_or_place(|| Button::new());
                    add.listen(|ctx, ClickEvent| {
                        ctx.get_mut().add_track();
                        ctx.get_mut().update();
                    });
                    add.child().get_or_place(|| Text::new("add".to_string(), style.clone()));
                }
            }

            {
                let mut notes = root.child().get_or_place(|| Row::new(5.0));
                for i in 0..self.song.notes.len() {
                    let mut col = notes.child().get_or_place(|| Col::new(5.0));

                    {
                        let mut inst = col.child().get_or_place(|| Button::new());
                        inst.listen(move |ctx, ClickEvent| {
                            if let Ok(result) = nfd::dialog().filter("wav").open() {
                                match result {
                                    nfd::Response::Okay(path) => {
                                        let wave = hound::WavReader::open(path).unwrap();
                                        let samples: Vec<f32> = match wave.spec().sample_format {
                                            hound::SampleFormat::Float => {
                                                wave.into_samples::<f32>().map(|s| s.unwrap()).collect()
                                            }
                                            hound::SampleFormat::Int => {
                                                wave.into_samples::<i32>().map(|s| s.unwrap() as f32 / 32768.0).collect()
                                            }
                                        };
                                        ctx.get_mut().song.samples[i] = samples;
                                        ctx.get_mut().update();
                                    }
                                    _ => {}
                                }
                            }
                        });
                        inst.child().get_or_place(|| Text::new("inst".to_string(), style.clone()));
                    }

                    {
                        let mut del = col.child().get_or_place(|| Button::new());
                        del.listen(move |ctx, ClickEvent| {
                            ctx.get_mut().delete_track(i);
                            ctx.get_mut().update();
                        });
                        del.child().get_or_place(|| Text::new("del".to_string(), style.clone()));
                    }

                    for j in 0..self.song.ptn_len {
                        let mut bg = col.child().get_or_place(|| BackgroundColor::new([0.0, 0.0, 0.0, 0.0]));
                        if (i,j) == self.cursor {
                            bg.get_mut().color([0.02, 0.2, 0.6, 1.0]);
                        } else {
                            bg.get_mut().color([0.0, 0.0, 0.0, 0.0]);
                        }
                        let mut note = bg.child().get_or_place(|| NoteElement::new(4, Note::None, style.clone()));
                        note.get_mut().value(self.song.notes[i][j].clone());
                        note.listen(move |ctx, value: Note| {
                            ctx.get_mut().song.notes[i][j] = value.clone();
                            ctx.get_mut().update();
                        });
                    }
                }
            }

            root.listen(|ctx, KeyPress(button)| {
                match button {
                    KeyboardButton::Up => { ctx.get_mut().cursor.1 = ctx.get().cursor.1.saturating_sub(1); }
                    KeyboardButton::Down => { ctx.get_mut().cursor.1 = (ctx.get().cursor.1 + 1).min(ctx.get().song.ptn_len.saturating_sub(1)); }
                    KeyboardButton::Left => { ctx.get_mut().cursor.0 = ctx.get().cursor.0.saturating_sub(1); }
                    KeyboardButton::Right => { ctx.get_mut().cursor.0 = (ctx.get().cursor.0 + 1).min(ctx.get().song.notes.len().saturating_sub(1)); }
                    KeyboardButton::Key1 | KeyboardButton::Key2 | KeyboardButton::Key3 | KeyboardButton::Key4 => {
                        let cursor = ctx.get().cursor;
                        match ctx.get().song.notes[cursor.0][cursor.1] {
                            Note::Off | Note::None => { ctx.get_mut().song.notes[cursor.0][cursor.1] = Note::On(vec![0; 4]); }
                            _ => {}
                        }

                        let delta = if ctx.get_input_state().modifiers.shift { -1 } else { 1 };
                        if let Note::On(ref mut factors) = ctx.get_mut().song.notes[cursor.0].get_mut(cursor.1).unwrap() {
                            match button {
                                KeyboardButton::Key1 => { factors[0] += delta; }
                                KeyboardButton::Key2 => { factors[1] += delta; }
                                KeyboardButton::Key3 => { factors[2] += delta; }
                                KeyboardButton::Key4 => { factors[3] += delta }
                                _ => {}
                            }
                        }

                        ctx.get_mut().update();
                    }
                    KeyboardButton::Back | KeyboardButton::Delete => {
                        let cursor = ctx.get().cursor;
                        ctx.get_mut().song.notes[cursor.0][cursor.1] = Note::None;
                        ctx.get_mut().update();
                    }
                    KeyboardButton::Grave | KeyboardButton::O => {
                        let cursor = ctx.get().cursor;
                        ctx.get_mut().song.notes[cursor.0][cursor.1] = Note::Off;
                        ctx.get_mut().update();
                    }
                    _ => {}
                }
            });
        }
    }
    ui.place(App {
         song: Song::default(),
         audio_send: start_audio_thread(),
         cursor: (0, 0),
         font: Rc::new(FontCollection::from_bytes(include_bytes!("../sawarabi-gothic-medium.ttf") as &[u8]).into_font().unwrap()),
    });

    renderer.render(ui.display());

    let mut cursor_hide = false;
    events_loop.run_forever(|ev| {
        let input_event = match ev {
            glutin::Event::WindowEvent { event, .. } => match event {
                glutin::WindowEvent::Closed => {
                    return glutin::ControlFlow::Break;
                }
                glutin::WindowEvent::Resized(w, h) => {
                    ui.resize(w as f32, h as f32);
                    renderer.render(ui.display());
                    None
                }
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    Some(InputEvent::MouseMove(x as f32, y as f32))
                }
                glutin::WindowEvent::MouseInput { device_id: _, state, button, modifiers } => {
                    ui.modifiers(KeyboardModifiers::from_glutin(modifiers));

                    let button = match button {
                        glutin::MouseButton::Left => Some(MouseButton::Left),
                        glutin::MouseButton::Middle => Some(MouseButton::Middle),
                        glutin::MouseButton::Right => Some(MouseButton::Right),
                        _ => None,
                    };

                    if let Some(button) = button {
                        match state {
                            glutin::ElementState::Pressed => {
                                Some(InputEvent::MousePress(button))
                            }
                            glutin::ElementState::Released => {
                                Some(InputEvent::MouseRelease(button))
                            }
                        }
                    } else {
                        None
                    }
                }
                glutin::WindowEvent::KeyboardInput { device_id: _, input } => {
                    ui.modifiers(KeyboardModifiers::from_glutin(input.modifiers));

                    if let Some(keycode) = input.virtual_keycode {
                        let button = KeyboardButton::from_glutin(keycode);

                        match input.state {
                            glutin::ElementState::Pressed => {
                                Some(InputEvent::KeyPress(button))
                            }
                            glutin::ElementState::Released => {
                                Some(InputEvent::KeyRelease(button))
                            }
                        }
                    } else {
                        None
                    }
                }
                glutin::WindowEvent::ReceivedCharacter(c) => {
                    Some(InputEvent::TextInput(c))
                }
                _ => None,
            },
            _ => None,
        };

        if let Some(input_event) = input_event {
            let response = ui.input(input_event);

            if let Some(point) = response.mouse_position {
                renderer.get_display().gl_window().set_cursor_position(point.x as i32, point.y as i32).expect("could not set cursor state");
            }

            if let Some(mouse_cursor) = response.mouse_cursor {
                renderer.get_display().gl_window().set_cursor(MouseCursor::to_glutin(mouse_cursor));
            }

            if let Some(hidden) = response.hide_cursor {
                if hidden {
                    if !cursor_hide {
                        renderer.get_display().gl_window().set_cursor_state(glutin::CursorState::Hide).expect("could not set cursor state");
                        cursor_hide = true;
                    }
                } else {
                    if cursor_hide {
                        renderer.get_display().gl_window().set_cursor_state(glutin::CursorState::Normal).expect("could not set cursor state");
                        cursor_hide = false;
                    }
                }
            }

            renderer.render(ui.display());
        }

        glutin::ControlFlow::Continue
    });
}


struct IntegerInput {
    value: i32,
    style: TextStyle,
    old: i32,
    delta: f32,
    drag_origin: (f32, f32),
    dragging: bool,
}

impl IntegerInput {
    fn new(value: i32, style: TextStyle) -> IntegerInput {
        IntegerInput {
            value,
            style,
            old: value,
            delta: 0.0,
            drag_origin: (0.0, 0.0),
            dragging: false,
        }
    }

    fn value(&mut self, value: i32) {
        self.value = value;
    }

    fn start_dragging(&mut self, mouse_position: (f32, f32)) {
        self.dragging = true;
        self.old = self.value;
        self.delta = 0.0;
        self.drag_origin = mouse_position;
    }

    fn drag(&mut self, mouse_position: (f32, f32)) -> i32 {
        self.delta -= (mouse_position.1 - self.drag_origin.1) / 8.0;
        self.value = (self.old as f32 + self.delta) as i32;
        self.value
    }
}

impl Component for IntegerInput {
    fn install(&self, context: &mut InstallContext<IntegerInput>, children: &[Child]) {
        let mut text = context.root().get_or_place(|| Text::new(self.value.to_string(), self.style.clone()));
        text.get_mut().text(self.value.to_string());
        text.listen(|ctx, MousePress(button)| {
            ctx.capture_mouse();
            ctx.hide_cursor();
            let mouse_position = ctx.get_mouse_position();
            ctx.get_mut().start_dragging(mouse_position);
        });
        text.listen(|ctx, MouseMove(x, y)| {
            if ctx.get().dragging {
                let mouse_position = ctx.get_mouse_position();
                let drag_origin = ctx.get().drag_origin;
                ctx.set_mouse_position(drag_origin.0, drag_origin.1);
                let previous = ctx.get().value;
                let value = ctx.get_mut().drag(mouse_position);
                if value != previous {
                    ctx.fire(value);
                }
            }
        });
        text.listen(|ctx, MouseRelease(button)| {
            ctx.release_mouse();
            ctx.show_cursor();
            ctx.get_mut().dragging = false;
        });
    }
}


struct NoteElement {
    num_factors: usize,
    value: Note,
    style: TextStyle,
}

impl NoteElement {
    fn new(num_factors: usize, value: Note, style: TextStyle) -> NoteElement {
        NoteElement { num_factors, value, style }
    }

    fn value(&mut self, value: Note) {
        self.value = value;
    }
}

impl Component for NoteElement {
    fn install(&self, context: &mut InstallContext<NoteElement>, children: &[Child]) {
        let mut padding = context.root().get_or_place(|| Padding::new(2.0));
        let mut row = padding.child().get_or_place(|| Row::new(5.0));

        match self.value {
            Note::On(ref factors) => {
                for i in 0..factors.len() {
                    let mut factor = row.child().get_or_place(|| IntegerInput::new(factors[i] as i32, self.style.clone()));
                    factor.get_mut().value(factors[i] as i32);
                    factor.listen(move |ctx, value: i32| {
                        if let Note::On(ref mut factors) = ctx.get_mut().value {
                            factors[i] = value;
                        }
                        let value = ctx.get().value.clone();
                        ctx.fire(value);
                    });
                }
            }
            Note::Off => {
                for _ in 0..self.num_factors {
                    row.child().place(Text::new("--".to_string(), self.style.clone()));
                }
            }
            Note::None => {
                for _ in 0..self.num_factors {
                    row.child().place(Text::new("..".to_string(), self.style.clone()));
                }
            }
        }
    }
}
