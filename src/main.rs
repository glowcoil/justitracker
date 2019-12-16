mod audio;

use audio::{Audio, Msg};

extern crate gl;
extern crate glfw;
extern crate gouache;
extern crate nfd;
extern crate portaudio;
extern crate sendai;

use gouache::*;
use gouache::renderers::GlRenderer;
use sendai::*;

use std::rc::Rc;

#[derive(Clone)]
pub struct Song {
    tracks: usize,
    length: usize,
    samples: Vec<Vec<f32>>,
    notes: Vec<Note>,
}

#[derive(Copy, Clone, Debug)]
pub enum Note {
    On([i32; 4]),
    Off,
    None,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            tracks: 8,
            length: 8,
            samples: vec![vec![0.0; 1]; 8],
            notes: vec![Note::None; 8 * 8],
        }
    }
}

struct Editor {
    rect: Rect,
    font: Rc<Font<'static>>,

    play: Button,
    textbox: Textbox,

    song: Song,
    cursor: (usize, usize),
    playing: bool,

    audio: Audio,
}

impl Default for Editor {
    fn default() -> Editor {
        let font = Rc::new(Font::from_bytes(include_bytes!("../res/SourceSansPro-Regular.ttf")).unwrap());

        let mut textbox = Textbox::new(font.clone());
        *textbox.text_mut() = String::from("text");

        Editor {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            font,

            play: Button::new(PathBuilder::new()
                .move_to(4.0, 3.0)
                .line_to(4.0, 13.0)
                .line_to(12.0, 8.0)
                .build()),
            textbox,

            song: Song::default(),
            cursor: (0, 0),
            playing: false,

            audio: Audio::start().unwrap(),
        }
    }
}

impl Component for Editor {
    fn layout(&mut self, rect: Rect) -> Rect {
        self.rect = rect;

        self.play.layout(Rect::new(0.0, 0.0, 16.0, 16.0));
        self.textbox.layout(Rect::new(20.0, 0.0, 128.0, 16.0));

        self.rect
    }

    fn render(&self, frame: &mut Frame) {
        self.play.render(frame);
        self.textbox.render(frame);

        let (cell_w, cell_h) = self.font.measure("00", 14.0);
        let (cell_w, cell_h) = (cell_w.ceil(), cell_h.ceil());
        let cell_spacing = 2.0;
        let toolbar_height = 24.0;
        let offset = Vec2::new(0.0, toolbar_height);
        for t in 0..self.song.tracks {
            for r in 0..self.song.length {
                let offset = offset + Vec2::new(4.0 * t as f32 * (cell_w + cell_spacing), r as f32 * (cell_h + cell_spacing));

                if self.cursor == (t, r) {
                    frame.draw_rect(
                        offset,
                        Vec2::new(4.0 * cell_w + 3.0 * cell_spacing, cell_h),
                        Mat2x2::id(),
                        Color::rgba(0.141, 0.44, 0.77, 1.0),
                    );
                }

                let note = self.song.notes[t * self.song.tracks + r];
                for f in 0..4 {
                    let text = match note {
                        Note::On(value) => format!("{:02}", value[f]),
                        Note::Off => "--".to_string(),
                        Note::None => ". .".to_string(),
                    };
                    frame.draw_text(
                        &self.font,
                        14.0,
                        &text,
                        offset + Vec2::new(f as f32 * (cell_w + cell_spacing), 0.0),
                        Mat2x2::id(),
                        Color::rgba(1.0, 1.0, 1.0, 1.0),
                    );
                }
            }
        }
    }

    fn handle(&mut self, event: Event, context: &mut Context) -> bool {
        match event {
            Event::KeyDown(key) => {
                match key {
                    Key::Left => { self.cursor.0 = self.cursor.0.saturating_sub(1) }
                    Key::Right => { self.cursor.0 = (self.cursor.0 + 1).min(self.song.tracks - 1) }
                    Key::Up => { self.cursor.1 = self.cursor.1.saturating_sub(1) }
                    Key::Down => { self.cursor.1 = (self.cursor.1 + 1).min(self.song.length - 1) }
                    Key::Key1 | Key::Key2 | Key::Key3 | Key::Key4 => {
                        let mut note = &mut self.song.notes[self.cursor.0 * self.song.length + self.cursor.1];
                        let mut value = if let Note::On(value) = note { *value } else { [0; 4] };
                        let inc = if context.modifiers.shift { -1 } else { 1 };
                        let idx = match key {
                            Key::Key1 => { value[0] += inc }
                            Key::Key2 => { value[1] += inc }
                            Key::Key3 => { value[2] += inc }
                            Key::Key4 => { value[3] += inc }
                            _ => {}
                        };
                        *note = Note::On(value);
                        self.audio.send(Msg::Song(self.song.clone()));
                    }
                    Key::GraveAccent => {
                        self.song.notes[self.cursor.0 * self.song.length + self.cursor.1] = Note::Off;
                        self.audio.send(Msg::Song(self.song.clone()));
                    }
                    Key::Backspace | Key::Delete => {
                        self.song.notes[self.cursor.0 * self.song.length + self.cursor.1] = Note::None;
                        self.audio.send(Msg::Song(self.song.clone()));
                    }
                    Key::I => {
                        if let Ok(nfd::Response::Okay(path)) = nfd::open_file_dialog(Some("wav"), None) {
                            if let Ok(mut reader) = hound::WavReader::open(path) {
                                self.song.samples[self.cursor.0] = match reader.spec().sample_format {
                                    hound::SampleFormat::Float => {
                                        reader.samples::<f32>().map(|s| s.unwrap() as f32).collect()
                                    }
                                    hound::SampleFormat::Int => {
                                        reader.samples::<i32>().map(|s| s.unwrap() as f32 / 32768.0).collect()
                                    }
                                };
                                self.audio.send(Msg::Song(self.song.clone()));
                            }
                        }
                    }
                    Key::Space => {
                        if self.playing {
                            self.playing = false;
                            self.audio.send(Msg::Stop);
                        } else {
                            self.playing = true;
                            self.audio.send(Msg::Play);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        if self.play.handle(event, context) {
            self.playing = true;
            self.audio.send(Msg::Play);
        }
        self.textbox.handle(event, context);

        false
    }
}

fn main() {
    let mut editor = Editor::default();
    backends::glfw::run(&mut editor);
}
