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
    let font = collection.into_font().unwrap();

    let style = ui.prop(TextStyle { font: font, scale: Scale::uniform(14.0) });

    let song = ui.prop(Song::default());
    let audio_send = start_audio_thread();
    let cursor = ui.prop((0, 0));

    let play = Button::with_text("play".to_string().into(), style.into()).install(&mut ui);
    let stop = Button::with_text("stop".to_string().into(), style.into()).install(&mut ui);
    let bpm_label = Text::new("bpm:".to_string().into(), style.into()).install(&mut ui);
    let bpm = ui.prop(120);
    let bpm_input = IntegerInput::new(bpm, style.into())
        .on_change(move |ctx, value| {
            ctx.get_mut(song).bpm = value as u32;
            audio_send.send(AudioMessage::Song(ctx.get(song).clone())).unwrap();
        })
        .install(&mut ui);
    let ptn_len_label = Text::new("len:".to_string().into(), style.into()).install(&mut ui);
    let ptn_len = ui.prop(8);
    let ptn_len_input = IntegerInput::new(ptn_len, style.into()).install(&mut ui);

    let controls = Row::new(5.0.into()).install(&mut ui, &[play, stop, bpm_label, bpm_input, ptn_len_label, ptn_len_input]);
    ui.listen(controls, |ctx, event| {
        // match event {
        //     InputEvent::KeyPress { button } => {
        //         match button {
        //             KeyboardButton::Up | KeyboardButton::Down | KeyboardButton::Left | KeyboardButton::Right => {
        //                 let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
        //                 ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.0, 0.0, 0.0, 0.0]));

        //                 match button {
        //                     KeyboardButton::Up => { self.cursor.1 = self.cursor.1.saturating_sub(1); }
        //                     KeyboardButton::Down => { self.cursor.1 = (self.cursor.1 + 1).min(self.song.ptn_length.saturating_sub(1)); }
        //                     KeyboardButton::Left => { self.cursor.0 = self.cursor.0.saturating_sub(1); }
        //                     KeyboardButton::Right => { self.cursor.0 = (self.cursor.0 + 1).min(self.song.notes.len().saturating_sub(1)); }
        //                     _ => {}
        //                 }

        //                 let cursor_note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
        //                 ctx.set_element_style::<BoxStyle>(cursor_note, BoxStyle::color([0.02, 0.2, 0.6, 1.0]));
        //             }
        //             KeyboardButton::Key1 | KeyboardButton::Key2 | KeyboardButton::Key3 | KeyboardButton::Key4 => {
        //                 match self.song.notes[self.cursor.0][self.cursor.1] {
        //                     Note::Off | Note::None => { self.song.notes[self.cursor.0][self.cursor.1] = Note::On(vec![0; 4]); }
        //                     _ => {}
        //                 }

        //                 let delta = if ctx.get_input_state().modifiers.shift { -1 } else { 1 };
        //                 if let Note::On(ref mut factors) = self.song.notes[self.cursor.0].get_mut(self.cursor.1).unwrap() {
        //                     match button {
        //                         KeyboardButton::Key1 => { factors[0] += delta; }
        //                         KeyboardButton::Key2 => { factors[1] += delta; }
        //                         KeyboardButton::Key3 => { factors[2] += delta; }
        //                         KeyboardButton::Key4 => { factors[3] += delta }
        //                         _ => {}
        //                     }
        //                 }

        //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
        //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

        //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
        //             }
        //             KeyboardButton::Back | KeyboardButton::Delete => {
        //                 self.song.notes[self.cursor.0][self.cursor.1] = Note::None;

        //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
        //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

        //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
        //             }
        //             KeyboardButton::Grave => {
        //                 self.song.notes[self.cursor.0][self.cursor.1] = Note::Off;

        //                 let note = ctx.get_slot(self.note_columns[self.cursor.0]).get_child(self.cursor.1).unwrap();
        //                 ctx.send::<Note>(note, self.song.notes[self.cursor.0][self.cursor.1].clone());

        //                 self.audio_send.send(AudioMessage::Song(self.song.clone())).unwrap();
        //             }
        //             _ => {}
        //         }
        //     }
        //     _ => {}
        // }
    });

    let tree = ui.tree(move |ctx| {
        let text = Text::new("asdf".to_string().into(), style.into()).install(ctx);
        Col::new(5.0.into()).install(ctx, &[controls, text])
    });

    ui.root(tree);

    // let root = ctx.subtree().add_child(Stack::install);

    // ctx.set_element_style::<StackStyle>(root, StackStyle::axis(Axis::Vertical));

    // let controls_row = ctx.get_slot(root).add_child(Stack::install);
    // ctx.set_element_style::<BoxStyle>(root, BoxStyle::v_align(Align::Center));

    // let play_button = ctx.get_slot(controls_row).add_child(Button::install);
    // ctx.get_slot(play_button).add_child(Label::with_text("play"));
    // ctx.listen(play_button, |myself: &mut Grid, ctx, evt: ClickEvent| myself.audio_send.send(AudioMessage::Play).unwrap());

    // let stop_button = ctx.get_slot(controls_row).add_child(Button::install);
    // ctx.get_slot(stop_button).add_child(Label::with_text("stop"));
    // ctx.listen(stop_button, |myself: &mut Grid, ctx, evt: ClickEvent| myself.audio_send.send(AudioMessage::Stop).unwrap());

    // let properties = ctx.get_slot(controls_row).add_child(Stack::install);
    // ctx.set_element_style::<BoxStyle>(properties, BoxStyle::padding(5.0));
    // ctx.set_element_style::<StackStyle>(properties, StackStyle::spacing(5.0));

    // let bpm_label = ctx.get_slot(properties).add_child(Label::with_text("bpm:"));
    // ctx.set_element_style::<BoxStyle>(bpm_label, BoxStyle::v_align(Align::Center));
    // let bpm = ctx.get_slot(properties).add_child(IntegerInput::with_value(120));
    // ctx.listen(bpm, move |myself: &mut Grid, mut ctx, value: i32| {
    //     myself.song.bpm = value as u32;
    //     ctx.send::<i32>(bpm, value);
    //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // });

    // let ptn_length_label = ctx.get_slot(properties).add_child(Label::with_text("length:"));
    // ctx.set_element_style::<BoxStyle>(ptn_length_label, BoxStyle::v_align(Align::Center));
    // let ptn_length = ctx.get_slot(properties).add_child(IntegerInput::with_value(song.ptn_length as i32));
    // ctx.listen(ptn_length, move |myself: &mut Grid, mut ctx, value: i32| {
    //     let new_ptn_length = value.max(1) as usize;

    //     if new_ptn_length < myself.song.ptn_length {
    //         for track in 0..myself.song.notes.len() {
    //             myself.song.notes[track].truncate(new_ptn_length);
    //             for _ in 0..myself.song.ptn_length.saturating_sub(new_ptn_length) {
    //                 ctx.get_slot(myself.note_columns[track]).remove_child(new_ptn_length);
    //             }
    //         }
    //     } else if new_ptn_length > myself.song.ptn_length {
    //         for track in 0..myself.song.notes.len() {
    //             myself.song.notes[track].resize(new_ptn_length, Note::None);
    //             for i in myself.song.ptn_length..new_ptn_length {
    //                 Grid::note(&mut ctx, track, i, myself.note_columns[track]);
    //             }
    //         }
    //     }

    //     myself.song.ptn_length = new_ptn_length;
    //     ctx.send::<i32>(ptn_length, new_ptn_length as i32);

    //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    // });

    // let song: Song = Song::default();
    // let cursor = (0, 0);

    // let tracks = ctx.get_slot(root).add_child(Stack::install);
    // let columns = ctx.get_slot(tracks).add_child(Stack::install);
    // let mut note_columns = Vec::new();
    // for i in 0..song.notes.len() {
    //     note_columns.push(Grid::column(&mut ctx, i, song.ptn_length, columns));
    // }

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

    // let id = ctx.get_self();
    // ctx.listen(id, Grid::handle);

    // fn column(ctx: &mut Context<Grid>, i: usize, ptn_length: usize, columns: ElementRef) -> ElementRef {
    //     let column = ctx.get_slot(columns).add_child(Stack::install);
    //     ctx.set_element_style::<StackStyle>(column, StackStyle::axis(Axis::Vertical));

    //     let buttons = ctx.get_slot(column).add_child(Stack::install);
    //     let load_sample_button = ctx.get_slot(buttons).add_child(Button::install);
    //     ctx.get_slot(load_sample_button).add_child(Label::with_text("inst"));
    //     ctx.listen(load_sample_button, move |myself: &mut Grid, ctx, evt: ClickEvent| {
    //         if let Ok(result) = nfd::dialog().filter("wav").open() {
    //             match result {
    //                 nfd::Response::Okay(path) => {
    //                     let samples: Vec<f32> = hound::WavReader::open(path).unwrap().samples::<f32>().map(|s| s.unwrap()).collect();
    //                     myself.song.samples[i] = samples;
    //                     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    //                 }
    //                 _ => {}
    //             }
    //         }
    //     });

    //     let del_button = ctx.get_slot(buttons).add_child(Button::install);
    //     ctx.get_slot(del_button).add_child(Label::with_text("del"));
    //     // ctx.listen(del_button, move |myself: &mut Grid, mut ctx, value: ClickEvent| {
    //     //     myself.song.samples.remove(i);
    //     //     myself.song.notes.remove(i);

    //     //     ctx.get_slot(myself.columns).remove_child(i);
    //     //     myself.note_columns.remove(i);

    //     //     myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    //     // });

    //     let note_column = ctx.get_slot(column).add_child(Stack::install);
    //     ctx.set_element_style::<StackStyle>(note_column, StackStyle::axis(Axis::Vertical).spacing(5.0));
    //     for j in 0..ptn_length {
    //         Grid::note(ctx, i, j, note_column);
    //     }

    //     note_column
    // }

    // fn note(ctx: &mut Context<Grid>, i: usize, j: usize, note_column: ElementRef) -> ElementRef {
    //     let note = ctx.get_slot(note_column).add_child(NoteElement::with_value(4, Note::None));
    //     ctx.listen(note, move |myself: &mut Grid, mut ctx, value: Note| {
    //         myself.song.notes[i][j] = value.clone();
    //         ctx.send::<Note>(note, value);

    //         myself.audio_send.send(AudioMessage::Song(myself.song.clone())).unwrap();
    //     });

    //     note
    // }


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


struct IntegerInput {
    value: Prop<i32>,
    style: Ref<TextStyle>,
    on_change: Option<Box<Fn(&mut EventContext, i32)>>,
}

impl IntegerInput {
    fn new(value: Prop<i32>, style: Ref<TextStyle>) -> IntegerInput {
        IntegerInput { value: value, style: style, on_change: None }
    }

    pub fn on_change<F: Fn(&mut EventContext, i32) + 'static>(mut self, on_change: F) -> IntegerInput {
        self.on_change = Some(Box::new(on_change));
        self
    }

    fn install(self, ui: &mut impl Install) -> ElementRef {
        let old_value = ui.prop(None);
        let drag_origin = ui.prop(None);

        let string = ui.map(self.value, |value| value.to_string());
        let text = Text::new(string.into(), self.style).install(ui);
        ui.listen(text, move |ctx, event| {
            match event {
                ElementEvent::MousePress(MouseButton::Left) => {
                    ctx.capture_mouse(text);
                    ctx.hide_cursor();

                    let value = *ctx.get(self.value);
                    ctx.set(old_value, Some(value));
                    let mouse_position = ctx.get_mouse_position();
                    ctx.set(drag_origin, Some(mouse_position));
                }
                ElementEvent::MouseMove(position) => {
                    if let Some(drag_origin) = *ctx.get(drag_origin) {
                        *ctx.get_mut(self.value) -= ((position.y - drag_origin.y) / 8.0) as i32;
                        ctx.set_mouse_position(drag_origin);

                        if let Some(ref on_change) = self.on_change {
                            let value = *ctx.get(self.value);
                            on_change(ctx, value);
                        }
                    }
                }
                ElementEvent::MouseRelease(MouseButton::Left) => {
                    ctx.relinquish_mouse(text);
                    ctx.show_cursor();

                    ctx.set(old_value, None);
                    ctx.set(drag_origin, None);
                }
                _ => {}
            }
        });
        text
    }
}


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