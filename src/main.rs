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
#[macro_use]
extern crate serde_derive;
extern crate bincode;

use std::time::{Instant, Duration};
use std::thread;

use glium::glutin;

use rusttype::{FontCollection, Font, Scale};

use std::sync::mpsc;
use std::rc::Rc;

use std::fs::File;
use std::io::{BufReader, BufWriter};

use render::*;
use ui::*;
use audio::*;

enum Message {
    Play,
    Stop,
    LoadSample(usize),
    SetNote { track: usize, note: usize, factor: usize, power: i32 },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Song {
    bpm: u32,
    ptn_len: usize,
    samples: Vec<Vec<f32>>,
    notes: Vec<Vec<Note>>,
}

#[derive(Clone, Serialize, Deserialize)]
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
        .with_dimensions(glutin::dpi::LogicalSize::new(800.0, 600.0))
        .with_title("justitracker");
    let context = glutin::ContextBuilder::new();
    let display = glium::Display::new(window, context, &events_loop).unwrap();

    let (width, height, dpi_factor) = {
        let window = display.gl_window();
        let size = window.get_inner_size().unwrap();
        let dpi_factor = window.get_hidpi_factor();
        (size.width, size.height, dpi_factor)
    };

    let mut renderer = Renderer::new(display, width as f32, height as f32, dpi_factor as f32);

    let mut ui = UI::new(width as f32, height as f32);

    ui.place(App::new());


    const FRAME: std::time::Duration = std::time::Duration::from_micros(1_000_000 / 60);
    let mut running = true;
    let mut render = true;
    while running {
        let now = Instant::now();

        if render || ui.is_animating() {
            renderer.render(ui.display(FRAME));
            render = false;
        }

        events_loop.poll_events(|ev| {
            let input_event = match ev {
                glutin::Event::WindowEvent { event, .. } => match event {
                    glutin::WindowEvent::CloseRequested => {
                        running = false;
                        return;
                    }
                    glutin::WindowEvent::Resized(size) => {
                        use glium::glutin::GlContext;
                        renderer.get_display().gl_window().resize(size.to_physical(dpi_factor));
                        ui.resize(size.width as f32, size.height as f32);
                        renderer.resize(size.width as f32, size.height as f32);
                        renderer.render(ui.display(Duration::new(0, 0)));
                        None
                    }
                    glutin::WindowEvent::CursorMoved { position: pos, .. } => {
                        Some(InputEvent::CursorMove(pos.x as f32, pos.y as f32))
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
                    glutin::WindowEvent::MouseWheel { delta, modifiers, .. } => {
                        ui.modifiers(KeyboardModifiers::from_glutin(modifiers));

                        match delta {
                            glutin::MouseScrollDelta::LineDelta(x, y) => { Some(InputEvent::MouseScroll(x * 48.0, y * 48.0)) }
                            glutin::MouseScrollDelta::PixelDelta(glutin::dpi::LogicalPosition { x, y }) => { Some(InputEvent::MouseScroll(x as f32, y as f32)) }
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
                glutin::Event::DeviceEvent { event: glutin::DeviceEvent::MouseMotion { delta }, .. } => {
                    Some(InputEvent::MouseMove(delta.0 as f32, delta.1 as f32))
                },
                _ => None,
            };

            if let Some(input_event) = input_event {
                render = true;

                let response = ui.input(input_event);

                renderer.get_display().gl_window().grab_cursor(response.capture_cursor).expect("unable to capture cursor");

                if let Some(mouse_position) = response.mouse_position {
                    renderer.get_display().gl_window().set_cursor_position(glutin::dpi::LogicalPosition::new(mouse_position.0 as f64, mouse_position.1 as f64))
                        .expect("unable to set cursor position");
                }

                if let Some(mouse_cursor) = response.mouse_cursor {
                    renderer.get_display().gl_window().set_cursor(MouseCursor::to_glutin(mouse_cursor));
                }

                if let Some(hidden) = response.hide_cursor {
                    renderer.get_display().gl_window().hide_cursor(hidden);
                }
            }
        });

        let elapsed = now.elapsed();
        if elapsed < FRAME {
            std::thread::sleep(FRAME - elapsed);
        }
    }
}


struct App {
    filename: Option<String>,
    song: Song,
    audio_send: mpsc::Sender<AudioMessage>,
    sample_rate: u32,
    cursor: (usize, usize),
    font: Rc<Font<'static>>,
}

impl App {
    fn new() -> App {
        let (sample_rate, audio_send) = start_audio_thread();

        App {
            filename: None,
            song: Song::default(),
            sample_rate: sample_rate,
            audio_send: audio_send,
            cursor: (0, 0),
            font: Rc::new(FontCollection::from_bytes(include_bytes!("../sawarabi-gothic-medium.ttf") as &[u8]).into_font().unwrap()),
        }
    }

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

    fn save(&self, filename: &str) -> bool {
        if let Ok(f) = File::create(&filename) {
            let mut writer = BufWriter::new(f);
            bincode::serialize_into(writer, &self.song).is_ok()
        } else {
            false
        }
    }

    fn save_as(&mut self, filename: String) {
        if self.save(&filename) {
            self.filename = Some(filename);
        }
    }

    fn export(&self, filename: String) {
        let mut engine = Engine::new(self.sample_rate, self.song.clone());
        let length = ((60.0 / self.song.bpm as f32) * self.sample_rate as f32) as usize * self.song.ptn_len;
        let mut output = vec![0.0; length];
        engine.calculate(&mut output[..]);

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        if let Ok(mut writer) = hound::WavWriter::create(filename, spec) {
            for sample in output {
                writer.write_sample((sample * 32768.0) as i32).unwrap();
            }
            writer.finalize();
        }
    }
}

impl Component for App {
    fn install(&self, context: &mut InstallContext<App>, _children: &[Child]) {
        let style = TextStyle { font: self.font.clone(), scale: Scale::uniform(19.0) };

        let mut root = context.root().place(Col::new(5.0));

        {
            let mut controls = root.child().place(Row::new(5.0));
            {
                let mut save = controls.child().place(Button::new());
                save.listen(|myself, ctx, ClickEvent| {
                    if myself.filename.is_some() {
                        myself.save(myself.filename.as_ref().unwrap());
                    } else {
                        if let Ok(result) = nfd::dialog_save().filter("ji").open() {
                            match result {
                                nfd::Response::Okay(path) => {
                                    myself.save_as(path);
                                }
                                _ => {}
                            }
                        }
                    }

                });
                save.child().place(Text::new("save".to_string(), style.clone()));
            }
            {
                let mut load = controls.child().place(Button::new());
                load.listen(|myself, ctx, ClickEvent| {
                    if let Ok(result) = nfd::dialog().filter("ji").open() {
                        match result {
                            nfd::Response::Okay(path) => {
                                if let Ok(f) = File::open(&path) {
                                    let mut reader = BufReader::new(f);
                                    if let Ok(song) = bincode::deserialize_from(reader) {
                                        myself.song = song;
                                        myself.update();
                                        myself.filename = Some(path);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                });
                load.child().place(Text::new("load".to_string(), style.clone()));
            }
            {
                let mut export = controls.child().place(Button::new());
                export.listen(|myself, ctx, ClickEvent| {
                    if let Ok(result) = nfd::dialog_save().filter("wav").open() {
                        match result {
                            nfd::Response::Okay(path) => {
                                myself.export(path);
                            }
                            _ => {}
                        }
                    }
                });
                export.child().place(Text::new("export".to_string(), style.clone()));
            }
            {
                let mut play = controls.child().place(Button::new());
                play.listen(|myself, ctx, ClickEvent| { myself.audio_send.send(AudioMessage::Play).unwrap() });
                play.child().place(Text::new("play".to_string(), style.clone()));
            }
            {
                let mut stop = controls.child().place(Button::new());
                stop.listen(|myself, ctx, ClickEvent| { myself.audio_send.send(AudioMessage::Stop).unwrap() });
                stop.child().place(Text::new("stop".to_string(), style.clone()));
            }
            controls.child().place(Text::new("bpm:".to_string(), style.clone()));
            {
                let mut bpm = controls.child().place(IntegerInput::new(self.song.bpm as i32, style.clone()));
                bpm.listen(|myself, ctx, value: i32| {
                    myself.song.bpm = value as u32;
                    myself.update();
                });
            }
            controls.child().place(Text::new("len:".to_string(), style.clone()));
            {
                let mut ptn_len = controls.child().place(IntegerInput::new(self.song.ptn_len as i32, style.clone()));
                ptn_len.listen(|myself, ctx, value: i32| {
                    let value = value.max(1) as usize;
                    myself.set_ptn_len(value);
                    myself.update();
                });
            }
            {
                let mut add = controls.child().place(Button::new());
                add.listen(|myself, ctx, ClickEvent| {
                    myself.add_track();
                    myself.update();
                });
                add.child().place(Text::new("add".to_string(), style.clone()));
            }
        }

        {
            let mut scrollbox = root.child().place(Scrollbox::new());

            let mut notes = scrollbox.child().place(Row::new(5.0));
            for i in 0..self.song.notes.len() {
                let mut col = notes.child().place(Col::new(5.0));

                {
                    let mut inst = col.child().place(Button::new());
                    inst.listen(move |myself, ctx, ClickEvent| {
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
                                    myself.song.samples[i] = samples;
                                    myself.update();
                                }
                                _ => {}
                            }
                        }
                    });
                    inst.child().place(Text::new("inst".to_string(), style.clone()));
                }

                {
                    let mut del = col.child().place(Button::new());
                    del.listen(move |myself, ctx, ClickEvent| {
                        myself.delete_track(i);
                        myself.update();
                    });
                    del.child().place(Text::new("del".to_string(), style.clone()));
                }

                for j in 0..self.song.ptn_len {
                    let color = if (i,j) == self.cursor {
                        [0.02, 0.2, 0.6, 1.0]
                    } else {
                        [0.0, 0.0, 0.0, 0.0]
                    };
                    let mut bg = col.child().place(BackgroundColor::new(color));
                    let mut note = bg.child().place(NoteElement::new(4, self.song.notes[i][j].clone(), style.clone()));
                    note.listen(move |myself, ctx, value: Note| {
                        myself.song.notes[i][j] = value.clone();
                        myself.update();
                    });
                }
            }
        }

        root.listen(|myself, ctx, KeyPress(button)| {
            match button {
                KeyboardButton::Up => { myself.cursor.1 = myself.cursor.1.saturating_sub(1); }
                KeyboardButton::Down => { myself.cursor.1 = (myself.cursor.1 + 1).min(myself.song.ptn_len.saturating_sub(1)); }
                KeyboardButton::Left => { myself.cursor.0 = myself.cursor.0.saturating_sub(1); }
                KeyboardButton::Right => { myself.cursor.0 = (myself.cursor.0 + 1).min(myself.song.notes.len().saturating_sub(1)); }
                KeyboardButton::Key1 | KeyboardButton::Key2 | KeyboardButton::Key3 | KeyboardButton::Key4 => {
                    let cursor = myself.cursor;
                    match myself.song.notes[cursor.0][cursor.1] {
                        Note::Off | Note::None => { myself.song.notes[cursor.0][cursor.1] = Note::On(vec![0; 4]); }
                        _ => {}
                    }

                    let delta = if ctx.get_input_state().modifiers.shift { -1 } else { 1 };
                    if let Note::On(ref mut factors) = myself.song.notes[cursor.0].get_mut(cursor.1).unwrap() {
                        match button {
                            KeyboardButton::Key1 => { factors[0] += delta; }
                            KeyboardButton::Key2 => { factors[1] += delta; }
                            KeyboardButton::Key3 => { factors[2] += delta; }
                            KeyboardButton::Key4 => { factors[3] += delta }
                            _ => {}
                        }
                    }

                    myself.update();
                }
                KeyboardButton::Back | KeyboardButton::Delete => {
                    let cursor = myself.cursor;
                    myself.song.notes[cursor.0][cursor.1] = Note::None;
                    myself.update();
                }
                KeyboardButton::Grave | KeyboardButton::O => {
                    let cursor = myself.cursor;
                    myself.song.notes[cursor.0][cursor.1] = Note::Off;
                    myself.update();
                }
                _ => {}
            }
        });
    }
}


struct IntegerInput {
    value: i32,
    style: TextStyle,
    old: i32,
    delta: f32,
    dragging: bool,
}

impl IntegerInput {
    fn new(value: i32, style: TextStyle) -> IntegerInput {
        IntegerInput {
            value,
            style,
            old: value,
            delta: 0.0,
            dragging: false,
        }
    }

    fn value(&mut self, value: i32) {
        self.value = value;
    }

    fn drag(&mut self, mouse_motion: (f32, f32)) -> i32 {
        self.delta += mouse_motion.1;
        self.value = (self.old as f32 - self.delta / 8.0) as i32;
        self.value
    }
}

impl Component for IntegerInput {
    fn reconcile(&mut self, new: IntegerInput) {
        self.value = new.value;
        self.style = new.style;
    }

    fn install(&self, context: &mut InstallContext<IntegerInput>, _children: &[Child]) {
        let mut text = context.root().place(Text::new(self.value.to_string(), self.style.clone()));
        text.listen(|myself, ctx, MousePress(_)| {
            ctx.capture_cursor();

            myself.dragging = true;
            myself.old = myself.value;
            myself.delta = 0.0;
        });
        text.listen(|myself, ctx, MouseMove(dx, dy)| {
            if myself.dragging {
                let previous = myself.value;
                let value = myself.drag((dx, dy));
                if value != previous {
                    ctx.fire(value);
                }
            }
        });
        text.listen(|myself, ctx, MouseRelease(_)| {
            ctx.release_cursor();

            myself.dragging = false;
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
    fn install(&self, context: &mut InstallContext<NoteElement>, _children: &[Child]) {
        let mut padding = context.root().place(Padding::new(2.0));
        let mut row = padding.child().place(Row::new(5.0));

        match self.value {
            Note::On(ref factors) => {
                for i in 0..factors.len() {
                    let mut factor = row.child().place(IntegerInput::new(factors[i] as i32, self.style.clone()));
                    factor.listen(move |myself, ctx, value: i32| {
                        if let Note::On(ref mut factors) = myself.value {
                            factors[i] = value;
                        }
                        let value = myself.value.clone();
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
