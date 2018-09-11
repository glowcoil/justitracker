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

use std::rc::Rc;

use glium::glutin;

use rusttype::{FontCollection, Font, Scale, point, PositionedGlyph};

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
    ptn_length: usize,
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
            ptn_length: 8,
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

    let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
    let font = Rc::new(collection.into_font().unwrap());

    ui.root(
        component(0i32, move |cmp| {
            element(cmp.map(|n| Padding { padding: *n as f32 })).children(vec![
                element(Constant::new(BackgroundColor { color: [0.0, 0.0, 0.1, 1.0] })).children(vec![
                    element(Constant::new(Padding { padding: 20.0 })).children(vec![
                        element(Constant::new(BackgroundColor { color: [1.0, 0.0, 0.1, 1.0] })).on(cmp, |cmp, ev, ctx| {
                            if let InputEvent::MousePress { button: MouseButton::Left } = ev {
                                *cmp += 1;
                            }
                            println!("{}", cmp);
                        }).children(vec![
                            // element(Constant::new(Text { text: "hello".into(), style: &TextStyle { font: font.clone(), scale: Scale::uniform(14.0) } })).into()
                        ]).into()
                    ]).into()
                ]).into()
            ]).into()
        })
    );

    // ui.root(Button::new(Text::new("));

    // // ui.set_global_element_style::<Label, BoxStyle>(BoxStyle::padding(5.0));
    // // ui.set_global_style::<StackStyle>(StackStyle::grow(Grow::Equal).spacing(5.0));

    // ui.set_global_element_style::<Button, BoxStyle>(BoxStyle::padding(5.0));

    // let root = ui.place_root(Grid::install);

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
            let response = ui.input(input_event);

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

            renderer.render(ui.display());
        }

        glutin::ControlFlow::Continue
    });
}

// struct Grid {
//     song: Song,
//     audio_send: mpsc::Sender<AudioMessage>,
//     cursor: (usize, usize),
//     columns: ElementRef,
//     note_columns: Vec<ElementRef>,
// }

// impl Grid {
//     fn install(mut ctx: Context<Grid>) -> Grid {
//         let song = Song::default();

//         let root = ctx.subtree().add_child(Stack::install);

//         ctx.set_element_style::<StackStyle>(root, StackStyle::axis(Axis::Vertical));

//         let controls_row = ctx.get_slot(root).add_child(Stack::install);
//         ctx.set_element_style::<BoxStyle>(root, BoxStyle::v_align(Align::Center));

//         let play_button = ctx.get_slot(controls_row).add_child(Button::install);
//         ctx.get_slot(play_button).add_child(Label::with_text("play"));
//         ctx.listen(play_button, |myself: &mut Grid, ctx, evt: ClickEvent| myself.audio_send.send(AudioMessage::Play).unwrap());

//         let stop_button = ctx.get_slot(controls_row).add_child(Button::install);
//         ctx.get_slot(stop_button).add_child(Label::with_text("stop"));
//         ctx.listen(stop_button, |myself: &mut Grid, ctx, evt: ClickEvent| myself.audio_send.send(AudioMessage::Stop).unwrap());

//         let properties = ctx.get_slot(controls_row).add_child(Stack::install);
//         ctx.set_element_style::<BoxStyle>(properties, BoxStyle::padding(5.0));
//         ctx.set_element_style::<StackStyle>(properties, StackStyle::spacing(5.0));

//         let bpm_label = ctx.get_slot(properties).add_child(Label::with_text("bpm:"));
//         ctx.set_element_style::<BoxStyle>(bpm_label, BoxStyle::v_align(Align::Center));
//         let bpm = ctx.get_slot(properties).add_child(IntegerInput::with_value(120));
//         ctx.listen(bpm, move |myself: &mut Grid, mut ctx, value: i32| {
//             myself.song.bpm = value as u32;
//             ctx.send::<i32>(bpm, value);
//             myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//         });
//         // ctx.set_element_style::<BoxStyle>(bpm, BoxStyle::v_align(Align::Center));

//         let ptn_length_label = ctx.get_slot(properties).add_child(Label::with_text("length:"));
//         ctx.set_element_style::<BoxStyle>(ptn_length_label, BoxStyle::v_align(Align::Center));
//         let ptn_length = ctx.get_slot(properties).add_child(IntegerInput::with_value(song.ptn_length as i32));
//         ctx.listen(ptn_length, move |myself: &mut Grid, mut ctx, value: i32| {
//             let new_ptn_length = value.max(1) as usize;

//             if new_ptn_length < myself.song.ptn_length {
//                 for track in 0..myself.song.notes.len() {
//                     myself.song.notes[track].truncate(new_ptn_length);
//                     for _ in 0..myself.song.ptn_length.saturating_sub(new_ptn_length) {
//                         ctx.get_slot(myself.note_columns[track]).remove_child(new_ptn_length);
//                     }
//                 }
//             } else if new_ptn_length > myself.song.ptn_length {
//                 for track in 0..myself.song.notes.len() {
//                     myself.song.notes[track].resize(new_ptn_length, Note::None);
//                     for i in myself.song.ptn_length..new_ptn_length {
//                         Grid::note(&mut ctx, track, i, myself.note_columns[track]);
//                     }
//                 }
//             }

//             myself.song.ptn_length = new_ptn_length;
//             ctx.send::<i32>(ptn_length, new_ptn_length as i32);

//             myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//         });

//         let song: Song = Default::default();
//         let cursor = (0, 0);

//         let tracks = ctx.get_slot(root).add_child(Stack::install);
//         let columns = ctx.get_slot(tracks).add_child(Stack::install);
//         let mut note_columns = Vec::new();
//         // ctx.set_element_style::<StackStyle>(columns, StackStyle::spacing(2.0));
//         for i in 0..song.notes.len() {
//             note_columns.push(Grid::column(&mut ctx, i, song.ptn_length, columns));
//         }

//         let add_column = ctx.get_slot(tracks).add_child(Stack::install);
//         ctx.set_element_style::<StackStyle>(add_column, StackStyle::axis(Axis::Vertical));
//         let add_button = ctx.get_slot(add_column).add_child(Button::install);
//         ctx.get_slot(add_button).add_child(Label::with_text("add"));
//         ctx.listen(add_button, move |myself: &mut Grid, mut ctx, value: ClickEvent| {
//             myself.song.samples.push(vec![0.0; 1]);
//             let mut track = Vec::with_capacity(myself.song.ptn_length);
//             track.resize(myself.song.ptn_length, Note::None);
//             myself.song.notes.push(track);

//             myself.note_columns.push(Grid::column(&mut ctx, myself.song.ptn_length, myself.song.ptn_length, myself.columns));

//             myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//         });

//         let cursor_note = ctx.get_slot(note_columns[cursor.0]).get_child(cursor.1).unwrap();
//         ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.02, 0.2, 0.6, 1.0]));

//         let id = ctx.get_self();
//         ctx.listen(id, Grid::handle);

//         Grid {
//             song: song,
//             audio_send: start_audio_thread(),
//             cursor: cursor,
//             columns: columns,
//             note_columns: note_columns,
//         }
//     }

//     fn column(ctx: &mut Context<Grid>, i: usize, ptn_length: usize, columns: ElementRef) -> ElementRef {
//         let column = ctx.get_slot(columns).add_child(Stack::install);
//         ctx.set_element_style::<StackStyle>(column, StackStyle::axis(Axis::Vertical));

//         let buttons = ctx.get_slot(column).add_child(Stack::install);
//         let load_sample_button = ctx.get_slot(buttons).add_child(Button::install);
//         ctx.get_slot(load_sample_button).add_child(Label::with_text("inst"));
//         ctx.listen(load_sample_button, move |myself: &mut Grid, ctx, evt: ClickEvent| {
//             if let Ok(result) = nfd::dialog().filter("wav").open() {
//                 match result {
//                     nfd::Response::Okay(path) => {
//                         let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
//                         myself.song.samples[i] = samples;
//                         myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//                     }
//                     _ => {}
//                 }
//             }
//         });

//         let del_button = ctx.get_slot(buttons).add_child(Button::install);
//         ctx.get_slot(del_button).add_child(Label::with_text("del"));
//         // ctx.listen(del_button, move |myself: &mut Grid, mut ctx, value: ClickEvent| {
//         //     myself.song.samples.remove(i);
//         //     myself.song.notes.remove(i);

//         //     ctx.get_slot(myself.columns).remove_child(i);
//         //     myself.note_columns.remove(i);

//         //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//         // });

//         let note_column = ctx.get_slot(column).add_child(Stack::install);
//         ctx.set_element_style::<StackStyle>(note_column, StackStyle::axis(Axis::Vertical).spacing(5.0));
//         for j in 0..ptn_length {
//             Grid::note(ctx, i, j, note_column);
//         }

//         note_column
//     }

//     fn note(ctx: &mut Context<Grid>, i: usize, j: usize, note_column: ElementRef) -> ElementRef {
//         let note = ctx.get_slot(note_column).add_child(NoteElement::with_value(4, Note::None));
//         ctx.listen(note, move |myself: &mut Grid, mut ctx, value: Note| {
//             myself.song.notes[i][j] = value.clone();
//             ctx.send::<Note>(note, value);

//             myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
//         });

//         note
//     }

//     fn handle(&mut self, mut ctx: Context<Grid>, evt: InputEvent) {
//         match evt {
//             InputEvent::KeyPress { button } => {
//                 match button {
//                     KeyboardButton::Up | KeyboardButton::Down | KeyboardButton::Left | KeyboardButton::Right => {
//                         let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
//                         ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.0, 0.0, 0.0, 0.0]));

//                         match button {
//                             KeyboardButton::Up => { self.cursor.1 = self.cursor.1.saturating_sub(1); }
//                             KeyboardButton::Down => { self.cursor.1 = (self.cursor.1 + 1).min(self.song.ptn_length.saturating_sub(1)); }
//                             KeyboardButton::Left => { self.cursor.0 = self.cursor.0.saturating_sub(1); }
//                             KeyboardButton::Right => { self.cursor.0 = (self.cursor.0 + 1).min(self.song.notes.len().saturating_sub(1)); }
//                             _ => {}
//                         }

//                         let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
//                         ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
//                     }
//                     KeyboardButton::Key1 | KeyboardButton::Key2 | KeyboardButton::Key3 | KeyboardButton::Key4 => {
//                         match self.song.notes[self.cursor.0][self.cursor.1] {
//                             Note::Off | Note::None => { self.song.notes[self.cursor.0][self.cursor.1] = Note::On(vec![0; 4]); }
//                             _ => {}
//                         }

//                         let delta = if ctx.get_input_state().modifiers.shift { -1 } else { 1 };
//                         if let Note::On(ref mut factors) = self.song.notes[self.cursor.0].get_mut(self.cursor.1).unwrap() {
//                             match button {
//                                 KeyboardButton::Key1 => { factors[0] += delta; }
//                                 KeyboardButton::Key2 => { factors[1] += delta; }
//                                 KeyboardButton::Key3 => { factors[2] += delta; }
//                                 KeyboardButton::Key4 => { factors[3] += delta }
//                                 _ => {}
//                             }
//                         }

//                         let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
//                         ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

//                         self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
//                     }
//                     KeyboardButton::Back | KeyboardButton::Delete => {
//                         self.song.notes[self.cursor.0][self.cursor.1] = Note::None;

//                         let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
//                         ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

//                         self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
//                     }
//                     KeyboardButton::Grave => {
//                         self.song.notes[self.cursor.0][self.cursor.1] = Note::Off;

//                         let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
//                         ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

//                         self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
//                     }
//                     _ => {}
//                 }
//             }
//             _ => {}
//         }
//     }
// }

// impl Element for Grid {}


// struct IntegerInput {
//     value: i32,
//     old_value: Option<i32>,
// }

// impl IntegerInput {
//     fn with_value(value: i32) -> impl FnOnce(Context<IntegerInput>) -> IntegerInput {
//         move |mut ctx| {
//             ctx.subtree().add_child(Label::with_text(IntegerInput::format(value)));

//             let id = ctx.get_self();
//             ctx.listen(id, IntegerInput::handle);

//             ctx.receive(|myself: &mut IntegerInput, mut ctx: Context<IntegerInput>, value: i32| {
//                 myself.value = value;
//                 let label = ctx.subtree().get_child(0).unwrap();
//                 ctx.send::<String>(label, IntegerInput::format(value));
//             });

//             IntegerInput {
//                 value: value,
//                 old_value: None,
//             }
//         }
//     }

//     fn format(value: i32) -> String {
//         format!("{:02}", value)
//     }

//     fn handle(&mut self, mut ctx: Context<IntegerInput>, evt: InputEvent) {
//         match evt {
//             InputEvent::CursorMoved { position } => {
//                 if let Some(mouse_drag_origin) = ctx.get_input_state().mouse_drag_origin {
//                     if self.old_value.is_none() {
//                         self.old_value = Some(self.value);
//                     }
//                     let dy = -(ctx.get_input_state().mouse_position.y - mouse_drag_origin.y);
//                     ctx.fire::<i32>(self.old_value.unwrap() + (dy / 8.0) as i32);
//                 }
//             }
//             InputEvent::MouseRelease { button: MouseButton::Left } => {
//                 self.old_value = None;
//             }
//             _ => {}
//         }
//     }
// }

// impl Element for IntegerInput {}


// struct NoteElement {
//     num_factors: usize,
//     value: Note,
//     stack: ElementRef,
// }

// impl NoteElement {
//     fn with_value(num_factors: usize, value: Note) -> impl FnOnce(Context<NoteElement>) -> NoteElement {
//         move |mut ctx| {
//             let stack = ctx.subtree().add_child(Stack::install);
//             ctx.set_element_style::<StackStyle>(stack, StackStyle::spacing(5.0));
//             ctx.set_element_style::<BoxStyle>(stack, BoxStyle::padding(2.0));

//             NoteElement::setup(&mut ctx, num_factors, &value, stack);

//             ctx.receive(|myself: &mut NoteElement, mut ctx: Context<NoteElement>, value: Note| {
//                 myself.value = value;
//                 for _ in 0..myself.num_factors {
//                     ctx.get_slot(myself.stack).remove_child(0);
//                 }
//                 NoteElement::setup(&mut ctx, myself.num_factors, &myself.value, myself.stack);
//             });

//             NoteElement {
//                 num_factors: num_factors,
//                 value: value,
//                 stack: stack,
//             }
//         }
//     }

//     fn setup(ctx: &mut Context<NoteElement>, num_factors: usize, value: &Note, stack: ElementRef) {
//         match value {
//             Note::On(factors) => {
//                 for i in 0..num_factors {
//                     let factor = ctx.get_slot(stack).add_child(IntegerInput::with_value(factors[i] as i32));
//                     ctx.listen(factor, move |myself: &mut NoteElement, mut ctx, value: i32| {
//                         if let Note::On(ref factors) = myself.value {
//                             let mut factors = factors.clone();
//                             factors[i] = value;
//                             ctx.fire(Note::On(factors));
//                         }
//                     });
//                 }
//             }
//             Note::Off => {
//                 for _ in 0..num_factors {
//                     ctx.get_slot(stack).add_child(Label::with_text("--"));
//                 }
//             }
//             Note::None => {
//                 for _ in 0..4 {
//                     ctx.get_slot(stack).add_child(Label::with_text(".."));
//                 }
//             }
//         }
//     }
// }

// impl Element for NoteElement {}