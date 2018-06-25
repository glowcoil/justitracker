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

use glium::glutin;

use rusttype::{FontCollection, Font};

use std::collections::VecDeque;
use std::rc::Rc;
use std::cell::RefCell;

use std::sync::mpsc;

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
    samples: Vec<Vec<f32>>,
    notes: Vec<Vec<Option<Vec<i32>>>>,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            bpm: 120,
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

    let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
    let font = collection.into_font().unwrap();

    let mut ui = UI::new(width as f32, height as f32);

    // ui.set_global_element_style::<Label, BoxStyle>(BoxStyle::padding(5.0));
    
    let font_resource = ui.add_resource::<Font<'static>>(font);
    ui.set_global_style::<TextStyle>(TextStyle::font(font_resource));

    // ui.set_global_style::<StackStyle>(StackStyle::grow(Grow::Equal).spacing(5.0));

    ui.set_global_element_style::<Button, BoxStyle>(BoxStyle::padding(5.0));

    let root = ui.place_root(Grid::install);

    ui.layout();
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
                    ui.layout();
                    renderer.render(ui.display());
                    None
                }
                glutin::WindowEvent::CursorMoved { position: (x, y), .. } => {
                    Some(InputEvent::CursorMoved { position: Point { x: x as f32, y: y as f32 } })
                }
                glutin::WindowEvent::MouseInput { device_id: _, state, button, modifiers } => {
                    ui.set_modifiers(KeyboardModifiers::from_glutin(modifiers));

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
                    ui.set_modifiers(KeyboardModifiers::from_glutin(input.modifiers));

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
            let response = ui.handle(input_event);

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

            ui.layout();
            renderer.render(ui.display());
        }

        glutin::ControlFlow::Continue
    });
}

struct Grid {
    song: Song,
    audio_send: mpsc::Sender<AudioMessage>,
    cursor: (usize, usize),
    note_grid: Vec<Vec<ElementRef>>,
}

impl Grid {
    fn install(mut ctx: Context<Grid>) -> Grid {
        let root = ctx.subtree().add_child(Stack::install);

        ctx.set_element_style::<StackStyle>(root, StackStyle::axis(Axis::Vertical));

        let controls_row = ctx.get_slot(root).add_child(Stack::install);
        ctx.set_element_style::<BoxStyle>(root, BoxStyle::v_align(Align::Center));

        let play_button = ctx.get_slot(controls_row).add_child(Button::install);
        ctx.get_slot(play_button).add_child(Label::with_text("play"));
        ctx.listen(play_button, |myself: &mut Grid, ctx, evt: &ClickEvent| myself.audio_send.send(AudioMessage::Play).unwrap());

        let stop_button = ctx.get_slot(controls_row).add_child(Button::install);
        ctx.get_slot(stop_button).add_child(Label::with_text("stop"));
        ctx.listen(stop_button, |myself: &mut Grid, ctx, evt: &ClickEvent| myself.audio_send.send(AudioMessage::Stop).unwrap());

        let bpm_label = ctx.get_slot(controls_row).add_child(Label::with_text("bpm:"));
        ctx.set_element_style::<BoxStyle>(bpm_label, BoxStyle::v_align(Align::Center));
        let bpm = ctx.get_slot(controls_row).add_child(IntegerInput::with_value(Some(120)));
        ctx.listen(bpm, |myself: &mut Grid, ctx, value: i32| {
            myself.song.bpm = value as u32;
            myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
        });
        // ctx.set_element_style::<BoxStyle>(bpm, BoxStyle::v_align(Align::Center));

        let song: Song = Default::default();
        let mut note_grid = Vec::with_capacity(song.notes.len());
        let cursor = (0, 0);

        let columns = ctx.get_slot(root).add_child(Stack::install);
        // ctx.set_element_style::<StackStyle>(columns, StackStyle::spacing(2.0));
        for i in 0..song.notes.len() {
            let column = ctx.get_slot(columns).add_child(Stack::install);
            ctx.set_element_style::<StackStyle>(column, StackStyle::axis(Axis::Vertical));

            let buttons = ctx.get_slot(column).add_child(Stack::install);
            let load_sample_button = ctx.get_slot(buttons).add_child(Button::install);
            ctx.get_slot(load_sample_button).add_child(Label::with_text("inst"));
            ctx.listen(load_sample_button, move |myself: &mut Grid, ctx, evt: &ClickEvent| {
                if let Ok(result) = nfd::dialog().filter("wav").open() {
                    match result {
                        nfd::Response::Okay(path) => {
                            let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
                            myself.song.samples[i] = samples;
                            myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
                        }
                        _ => {}
                    }
                }
            });

            let del_button = ctx.get_slot(buttons).add_child(Button::install);
            ctx.get_slot(del_button).add_child(Label::with_text("del"));

            note_grid.push(Vec::with_capacity(song.notes[0].len()));
            let note_column = ctx.get_slot(column).add_child(Stack::install);
            ctx.set_element_style::<StackStyle>(note_column, StackStyle::axis(Axis::Vertical).spacing(5.0));
            for j in 0..song.notes[0].len() {
                let note = ctx.get_slot(note_column).add_child(Stack::install);
                for k in 0..4 {
                    let factor = ctx.get_slot(note).add_child(IntegerInput::with_value(None));
                    ctx.listen(factor, move |myself: &mut Grid, ctx, evt: i32| {
                        if myself.song.notes[i][j].is_none() {
                            myself.song.notes[i][j] = Some(vec![0; 4]);
                        }
                        if let Some(ref mut factors) = myself.song.notes[i][j] {
                            factors[k] = evt;
                        }
                        myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
                    });
                }
                ctx.set_element_style::<StackStyle>(note, StackStyle::spacing(5.0));
                ctx.set_element_style::<BoxStyle>(note, BoxStyle::padding(2.0));

                note_grid[i].push(note);
            }
        }
        let add_button = ctx.get_slot(columns).add_child(Button::install);
        ctx.get_slot(add_button).add_child(Label::with_text("add"));

        ctx.set_element_style::<BoxStyle>(note_grid[cursor.0][cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));

        ctx.receive(Grid::handle);

        Grid {
            song: Default::default(),
            audio_send: start_audio_thread(),
            cursor: cursor,
            note_grid: note_grid,
        }
    }

    fn handle(&mut self, mut ctx: Context<Grid>, evt: InputEvent) {
        match evt {
            InputEvent::KeyPress { button } => {
                match button {
                    KeyboardButton::Up => {
                        if self.cursor.1 > 0 {
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.0, 0.0, 0.0, 0.0]));
                            self.cursor.1 -= 1;
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
                        }
                    }
                    KeyboardButton::Down => {
                        if self.cursor.1 < self.note_grid[0].len().checked_sub(1).unwrap_or(0) {
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.0, 0.0, 0.0, 0.0]));
                            self.cursor.1 += 1;
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
                        }
                    }
                    KeyboardButton::Left => {
                        if self.cursor.0 > 0 {
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.0, 0.0, 0.0, 0.0]));
                            self.cursor.0 -= 1;
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
                        }
                    }
                    KeyboardButton::Right => {
                        if self.cursor.0 < self.note_grid.len().checked_sub(1).unwrap_or(0) {
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.0, 0.0, 0.0, 0.0]));
                            self.cursor.0 += 1;
                            ctx.set_element_style::<BoxStyle>(self.note_grid[self.cursor.0][self.cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
                        }

                    }
                    KeyboardButton::Key1 => {
                        if self.song.notes[self.cursor.0][self.cursor.1].is_none() {
                            self.song.notes[self.cursor.0][self.cursor.1] = Some(vec![0, 0, 0, 0]);
                        }
                        match self.song.notes[self.cursor.0][self.cursor.1].as_mut() {
                            Some(note) => {
                                if ctx.get_input_state().modifiers.shift {
                                    note[0] -= 1;
                                } else {
                                    note[0] += 1;
                                }
                                let factor = ctx.get_slot(self.note_grid[self.cursor.0][self.cursor.1]).get_child(0).unwrap();
                                ctx.send::<Option<i32>>(factor, Some(note[0]));
                            }
                            None => {}
                        }
                        self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
                    }
                    KeyboardButton::Key2 => {
                        if self.song.notes[self.cursor.0][self.cursor.1].is_none() {
                            self.song.notes[self.cursor.0][self.cursor.1] = Some(vec![0, 0, 0, 0]);
                        }
                        match self.song.notes[self.cursor.0][self.cursor.1].as_mut() {
                            Some(note) => {
                                if ctx.get_input_state().modifiers.shift {
                                    note[1] -= 1;
                                } else {
                                    note[1] += 1;
                                }
                                let factor = ctx.get_slot(self.note_grid[self.cursor.0][self.cursor.1]).get_child(1).unwrap();
                                ctx.send::<Option<i32>>(factor, Some(note[1]));
                            }
                            None => {}
                        }
                        self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
                    }
                    KeyboardButton::Key3 => {
                        if self.song.notes[self.cursor.0][self.cursor.1].is_none() {
                            self.song.notes[self.cursor.0][self.cursor.1] = Some(vec![0, 0, 0, 0]);
                        }
                        match self.song.notes[self.cursor.0][self.cursor.1].as_mut() {
                            Some(note) => {
                                if ctx.get_input_state().modifiers.shift {
                                    note[2] -= 1;
                                } else {
                                    note[2] += 1;
                                }
                                let factor = ctx.get_slot(self.note_grid[self.cursor.0][self.cursor.1]).get_child(2).unwrap();
                                ctx.send::<Option<i32>>(factor, Some(note[2]));
                            }
                            None => {}
                        }
                        self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
                    }
                    KeyboardButton::Key4 => {
                        if self.song.notes[self.cursor.0][self.cursor.1].is_none() {
                            self.song.notes[self.cursor.0][self.cursor.1] = Some(vec![0, 0, 0, 0]);
                        }
                        match self.song.notes[self.cursor.0][self.cursor.1].as_mut() {
                            Some(note) => {
                                if ctx.get_input_state().modifiers.shift {
                                    note[3] -= 1;
                                } else {
                                    note[3] += 1;
                                }
                                let factor = ctx.get_slot(self.note_grid[self.cursor.0][self.cursor.1]).get_child(3).unwrap();
                                ctx.send::<Option<i32>>(factor, Some(note[3]));
                            }
                            None => {}
                        }
                        self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl Element for Grid {}


struct IntegerInput {
    value: Option<i32>,
    old_value: Option<i32>,
}

impl IntegerInput {
    fn with_value(value: Option<i32>) -> impl FnOnce(Context<IntegerInput>) -> IntegerInput {
        move |mut ctx| {
            ctx.subtree().add_child(Label::with_text(IntegerInput::format(value)));

            ctx.receive(|myself: &mut IntegerInput, mut ctx: Context<IntegerInput>, value: Option<i32>| {
                myself.value = value;
                let label = ctx.subtree().get_child(0).unwrap();
                ctx.send(label, IntegerInput::format(value));
            });

            ctx.receive(IntegerInput::handle);

            IntegerInput {
                value: value,
                old_value: None,
            }
        }
    }

    fn format(value: Option<i32>) -> String {
        value.map(|v| format!("{:02}", v)).unwrap_or("..".to_string())
    }

    fn handle(&mut self, mut ctx: Context<IntegerInput>, evt: InputEvent) {
        match evt {
            InputEvent::CursorMoved { position } => {
                if let Some(mouse_drag_origin) = ctx.get_input_state().mouse_drag_origin {
                    if self.value.is_none() {
                        self.value = Some(0);
                    }
                    if self.old_value.is_none() {
                        self.old_value = self.value;
                    }
                    let dy = -(ctx.get_input_state().mouse_position.y - mouse_drag_origin.y);
                    self.value = self.old_value.map(|v| v + (dy / 8.0) as i32);

                    let label = ctx.subtree().get_child(0).unwrap();
                    ctx.send(label, IntegerInput::format(self.value));
                    ctx.fire::<i32>(self.value.unwrap());
                }
            }
            InputEvent::MouseRelease { button: MouseButton::Left } => {
                self.old_value = None;
            }
            _ => {}
        }
    }
}

impl Element for IntegerInput {}
