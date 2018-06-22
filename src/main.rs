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

    let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
    let font = collection.into_font().unwrap();

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
            let bpm_label = ctx.get_slot(controls_row).add_child(Label::with_text("120"));

            let song: Song = Default::default();
            let mut note_grid = Vec::with_capacity(song.notes.len());
            let cursor = (0, 0);

            let columns = ctx.get_slot(root).add_child(Stack::install);
            ctx.set_element_style::<StackStyle>(columns, StackStyle::spacing(2.0));
            for i in 0..song.notes.len() {
                let column = ctx.get_slot(columns).add_child(Stack::install);
                ctx.set_element_style::<StackStyle>(column, StackStyle::axis(Axis::Vertical));

                let buttons = ctx.get_slot(column).add_child(Stack::install);
                let load_sample_button = ctx.get_slot(buttons).add_child(Button::install);
                ctx.get_slot(load_sample_button).add_child(Label::with_text("inst"));
                ctx.listen(load_sample_button, |myself: &mut Grid, ctx, evt: &ClickEvent| {
                    if let Ok(result) = nfd::dialog().filter("wav").open() {
                        match result {
                            nfd::Response::Okay(path) => {
                                let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
                                myself.song.samples[0] = samples;
                                myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
                            }
                            _ => {}
                        }
                    }
                });

                let del_button = ctx.get_slot(buttons).add_child(Button::install);
                ctx.get_slot(del_button).add_child(Label::with_text("del"));


                note_grid.push(Vec::with_capacity(song.notes[0].len()));
                for j in 0..song.notes[0].len() {
                    let note = ctx.get_slot(column).add_child(Stack::install);
                    ctx.set_element_style::<BoxStyle>(column, BoxStyle::max_width(80.0));
                    for k in 0..4 {
                        let factor = ctx.get_slot(note).add_child(Label::with_text("00"));
                    }

                    note_grid[i].push(note);
                }
            }
            let add_button = ctx.get_slot(columns).add_child(Button::install);
            ctx.get_slot(add_button).add_child(Label::with_text("add"));

            ctx.set_element_style::<BoxStyle>(note_grid[cursor.0][cursor.1], BoxStyle::color([0.02, 0.2, 0.6, 1.0]));

            ctx.receive::<InputEvent>(Grid::handle);

            Grid {
                song: Default::default(),
                audio_send: start_audio_thread(),
                cursor: cursor,
                note_grid: note_grid,
            }
        }

        fn handle(&mut self, mut ctx: Context<Grid>, evt: &InputEvent) {
            match *evt {
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
                                    // factor_grid[self.cursor.0][self.cursor.1][0].borrow_mut().set_value(note[0]);
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
                                    // factor_grid[self.cursor.0][self.cursor.1][1].borrow_mut().set_value(note[1]);
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
                                    // factor_grid[self.cursor.0][self.cursor.1][2].borrow_mut().set_value(note[2]);
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
                                    // factor_grid[self.cursor.0][self.cursor.1][3].borrow_mut().set_value(note[3]);
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

    impl Element for Grid {
        fn measure(&self, resources: &Resources, children: &[BoundingBox]) -> BoundingBox {
            let box_style = resources.get_style::<BoxStyle>();

            let mut width = 0.0f32;
            let mut height = 0.0f32;
            for child_box in children {
                width = width.max(child_box.size.x);
                height = height.max(child_box.size.y);
            }

            // width += 2.0 * box_style.padding;
            // height += 2.0 * box_style.padding;

            BoundingBox { pos: Point::new(0.0, 0.0), size: Point::new(width, height) }
        }

        fn arrange(&mut self, resources: &Resources, bounds: BoundingBox, children: &mut [BoundingBox]) -> BoundingBox {
            let box_style = resources.get_style::<BoxStyle>();

            for child_box in children.iter_mut() {
                child_box.pos = bounds.pos;
                // child_box.pos.x += box_style.padding;
                // child_box.pos.y += box_style.padding;
                child_box.size.x = bounds.size.x;// - box_style.padding * 2.0;
                child_box.size.y = bounds.size.y;// - box_style.padding * 2.0;
            }

            bounds
        }
    }

    let mut ui = UI::new(width as f32, height as f32);

    // ui.set_global_style::<BoxStyle>(BoxStyle::padding(10.0));
    
    let font_resource = ui.add_resource::<Font<'static>>(font);
    ui.set_global_style::<TextStyle>(TextStyle::font(font_resource));

    // ui.set_global_style::<StackStyle>(StackStyle::grow(Grow::Equal).spacing(5.0));

    ui.set_global_element_style::<Button, BoxStyle>(BoxStyle::padding(10.0));

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
