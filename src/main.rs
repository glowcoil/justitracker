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

use rusttype::{FontCollection, Scale};

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

    let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
    let font = collection.into_font().unwrap();

    let style = ui.prop(TextStyle { font: font, scale: Scale::uniform(14.0) });

    // let song = ui.prop(Song::default());
    // let audio_send = start_audio_thread();
    // let cursor = ui.prop((0, 0));

    let play = Button::with_text(&mut ui, "play".to_string().into(), style.into());
    let stop = Button::with_text(&mut ui, "stop".to_string().into(), style.into());
    // let bpm_label = Text::new("bpm:".to_string().into(), style.into()).install(&mut ui);
    // let bpm = ui.prop(120);
    // let bpm_input = IntegerInput::new(bpm, style.into())
    //     .on_change({
    //         let audio_send = audio_send.clone();
    //         move |ctx, value| {
    //             ctx.get_mut(song).bpm = value as u32;
    //             audio_send.send(AudioMessage::Song(ctx.get(song).clone())).unwrap();
    //         }
    //     })
    //     .install(&mut ui);
    // let ptn_len_label = Text::new("len:".to_string().into(), style.into()).install(&mut ui);
    // let ptn_len = ui.prop(8);
    // let ptn_len_input = IntegerInput::new(ptn_len, style.into())
    //     .on_change({
    //         let audio_send = audio_send.clone();
    //         move |ctx, value| {
    //             let value = value.max(1) as usize;
    //             ctx.set(ptn_len, value as i32);

    //             if value < ctx.get(song).ptn_len {
    //                 for track in 0..ctx.get(song).notes.len() {
    //                     ctx.get_mut(song).notes[track].truncate(value);
    //                 }
    //             } else if value > ctx.get(song).ptn_len {
    //                 for track in 0..ctx.get(song).notes.len() {
    //                     ctx.get_mut(song).notes[track].resize(value, Note::None);
    //                 }
    //             }

    //             ctx.get_mut(song).ptn_len = value;

    //             audio_send.send(AudioMessage::Song(ctx.get(song).clone())).unwrap();
    //         }
    //     })
    //     .install(&mut ui);

    let controls = Row::new(5.0.into()).install(&mut ui, &[play, stop]);//, bpm_label, bpm_input, ptn_len_label, ptn_len_input]);

    // // for i in 0..song.notes.len() {
    // //     let load_sample_button = Button::new("inst".to_string().into(), style)
    // //         .on_click({
    // //             let audio_send = audio_send.clone();
    // //             move |ctx| {
    // //                 if let Ok(result) = nfd::dialog().filter("wav").open() {
    // //                     match result {
    // //                         nfd::Response::Okay(path) => {
    // //                             let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
    // //                             ctx.get_mut(song).samples[i] = samples;
    // //                             audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // //                         }
    // //                         _ => {}
    // //                     }
    // //                 }
    // //             }
    // //         })
    // //         .install(&mut ui);

    // //     // let del_button = ctx.get_slot(buttons).add_child(Button::install);
    // //     // ctx.get_slot(del_button).add_child(Label::with_text("del"));
    // //     // ctx.listen(del_button, move |myself: &mut Grid, mut ctx, value: ClickEvent| {
    // //     //     myself.song.samples.remove(i);
    // //     //     myself.song.notes.remove(i);

    // //     //     ctx.get_slot(myself.columns).remove_child(i);
    // //     //     myself.note_columns.remove(i);

    // //     //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // //     // });

    // //     for j in 0..ptn_len {
    // //         let note = ctx.get_slot(note_column).add_child(NoteElement::with_value(4, Note::None));
    // //         ctx.listen(note, move |myself: &mut Grid, mut ctx, value: Note| {
    // //             myself.song.notes[i][j] = value.clone();
    // //             ctx.send::<Note>(note, value);

    // //             myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // //         });
    // //     }
    // //     let note_column = Col::new(5.0.into()).install(&mut ui, &[]);
    // // }

    let root = Col::new(5.0.into()).install(&mut ui, &[controls]);
    // ui.listen(root, |ctx, event| {
    //     // match event {
    //     //     InputEvent::KeyPress { button } => {
    //     //         match button {
    //     //             KeyboardButton::Up | KeyboardButton::Down | KeyboardButton::Left | KeyboardButton::Right => {
    //     //                 let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
    //     //                 ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.0, 0.0, 0.0, 0.0]));

    //     //                 match button {
    //     //                     KeyboardButton::Up => { self.cursor.1 = self.cursor.1.saturating_sub(1); }
    //     //                     KeyboardButton::Down => { self.cursor.1 = (self.cursor.1 + 1).min(self.song.ptn_length.saturating_sub(1)); }
    //     //                     KeyboardButton::Left => { self.cursor.0 = self.cursor.0.saturating_sub(1); }
    //     //                     KeyboardButton::Right => { self.cursor.0 = (self.cursor.0 + 1).min(self.song.notes.len().saturating_sub(1)); }
    //     //                     _ => {}
    //     //                 }

    //     //                 let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
    //     //                 ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
    //     //             }
    //     //             KeyboardButton::Key1 | KeyboardButton::Key2 | KeyboardButton::Key3 | KeyboardButton::Key4 => {
    //     //                 match self.song.notes[self.cursor.0][self.cursor.1] {
    //     //                     Note::Off | Note::None => { self.song.notes[self.cursor.0][self.cursor.1] = Note::On(vec![0; 4]); }
    //     //                     _ => {}
    //     //                 }

    //     //                 let delta = if ctx.get_input_state().modifiers.shift { -1 } else { 1 };
    //     //                 if let Note::On(ref mut factors) = self.song.notes[self.cursor.0].get_mut(self.cursor.1).unwrap() {
    //     //                     match button {
    //     //                         KeyboardButton::Key1 => { factors[0] += delta; }
    //     //                         KeyboardButton::Key2 => { factors[1] += delta; }
    //     //                         KeyboardButton::Key3 => { factors[2] += delta; }
    //     //                         KeyboardButton::Key4 => { factors[3] += delta }
    //     //                         _ => {}
    //     //                     }
    //     //                 }

    //     //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
    //     //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

    //     //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
    //     //             }
    //     //             KeyboardButton::Back | KeyboardButton::Delete => {
    //     //                 self.song.notes[self.cursor.0][self.cursor.1] = Note::None;

    //     //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
    //     //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

    //     //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
    //     //             }
    //     //             KeyboardButton::Grave => {
    //     //                 self.song.notes[self.cursor.0][self.cursor.1] = Note::Off;

    //     //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
    //     //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

    //     //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
    //     //             }
    //     //             _ => {}
    //     //         }
    //     //     }
    //     //     _ => {}
    //     // }
    // });

    ui.root(controls);


    // let add_column = ctx.get_slot(tracks).add_child(Stack::install);
    // ctx.set_element_style::<StackStyle>(add_column, StackStyle::axis(Axis::Vertical));
    // let add_button = ctx.get_slot(add_column).add_child(Button::install);
    // ctx.get_slot(add_button).add_child(Label::with_text("add"));
    // ctx.listen(add_button, move |myself: &mut Grid, mut ctx, value: ClickEvent| {
    //     myself.song.samples.push(vec![0.0; 1]);
    //     let mut track = Vec::with_capacity(myself.song.ptn_length);
    //     track.resize(myself.song.ptn_length, Note::None);
    //     myself.song.notes.push(track);

    //     myself.note_columns.push(Grid::column(&mut ctx, myself.song.ptn_length, myself.song.ptn_length, myself.columns));

    //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // });

    // let cursor_note = ctx.get_slot(note_columns[cursor.0]).get_child(cursor.1).unwrap();
    // ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.02, 0.2, 0.6, 1.0]));


    ui.update();
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
                    Some(InputEvent::MouseMove(Point { x: x as f32, y: y as f32 }))
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

            ui.update();

            renderer.render(ui.display());
        }

        glutin::ControlFlow::Continue
    });
}


// struct IntegerInput {
//     value: Prop<i32>,
//     style: Ref<TextStyle>,
//     on_change: Option<Box<Fn(&mut EventContext, i32)>>,
// }

// impl IntegerInput {
//     fn new(value: Prop<i32>, style: Ref<TextStyle>) -> IntegerInput {
//         IntegerInput { value: value, style: style, on_change: None }
//     }

//     pub fn on_change<F: Fn(&mut EventContext, i32) + 'static>(mut self, on_change: F) -> IntegerInput {
//         self.on_change = Some(Box::new(on_change));
//         self
//     }

//     fn install(self, ui: &mut impl Install) -> ElementRef {
//         let old = ui.prop(None);
//         let delta = ui.prop(None);
//         let drag_origin = ui.prop(None);

//         let string = ui.map(self.value, |value| value.to_string());
//         let text = Text::new(string.into(), self.style).install(ui);
//         ui.listen(text, move |ctx, event| {
//             match event {
//                 ElementEvent::MousePress(MouseButton::Left) => {
//                     ctx.capture_mouse(text);
//                     ctx.hide_cursor();

//                     let value = *ctx.get(self.value);
//                     ctx.set(old, Some(value));
//                     ctx.set(delta, Some(0.0));
//                     let mouse_position = ctx.get_mouse_position();
//                     ctx.set(drag_origin, Some(mouse_position));
//                 }
//                 ElementEvent::MouseMove(position) => {
//                     if let Some(drag_origin) = *ctx.get(drag_origin) {
//                         let old_value = ctx.get(old).unwrap();
//                         let delta_value = ctx.get(delta).unwrap() - (position.y - drag_origin.y) / 8.0;
//                         ctx.set(delta, Some(delta_value));
//                         ctx.set(self.value, (old_value as f32 + delta_value) as i32);
//                         ctx.set_mouse_position(drag_origin);

//                         if let Some(ref on_change) = self.on_change {
//                             let value = *ctx.get(self.value);
//                             on_change(ctx, value);
//                         }
//                     }
//                 }
//                 ElementEvent::MouseRelease(MouseButton::Left) => {
//                     ctx.relinquish_mouse(text);
//                     ctx.show_cursor();

//                     ctx.set(old, None);
//                     ctx.set(delta, None);
//                     ctx.set(drag_origin, None);
//                 }
//                 _ => {}
//             }
//         });
//         text
//     }
// }


// struct NoteElement {
//     num_factors: usize,
//     value: Prop<Note>,
//     style: Ref<TextStyle>,
// }

// impl NoteElement {
//     fn new(num_factors: usize, value: Prop<Note>, style: Ref<TextStyle>) -> NoteElement {
//         NoteElement {
//             num_factors: num_factors,
//             value: value,
//         }
//     }

//     fn install(self, ui: &mut UI) -> ElementRef {
//         let factors = Vec::new();
//         for i in 0..self.num_factors {
//             factors.push(IntegerInput::new(self.factors[i] as i32, self.style)
//                 .on_change(move |ctx, new_value| {
//                     if let Note::On(ref factors) = ctx.get(value) {
//                         factors[i] = value;
//                     }
//                 })
//                 .install(ui));
//         }
//         let factors = Row::new(5.0.into()).install(ui, &factors);

//         let mut off = Vec::new();
//         for _ in 0..num_factors {
//             off.push(Text::new("--".to_string().into(), self.style).install(ui));
//         }
//         let off = Row::new(5.0.into()).install(ui, &off);

//         let mut none = Vec::new();
//         for _ in 0..num_factors {
//             none.push(Text::new("..".to_string().into(), self.style).install(ui));
//         }
//         let none = Row::new(5.0.into()).install(ui, &none);

//         let contents = ui.tree(move |ctx| {
//             match self.value {
//                 Note::On(_) => {
//                     factors
//                 }
//                 Note::Off => {
//                     off
//                 }
//                 Note::None => {
//                     none
//                 }
//             }
//         });
//         Padding::new(2.0.into()).install(ui, contents)
//     }
// }
