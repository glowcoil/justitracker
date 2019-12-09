mod audio;
mod window;

use audio::{Audio, Msg};
use window::Window;

extern crate gl;
extern crate glfw;
extern crate gouache;
extern crate nfd;
extern crate portaudio;

use glfw::{Action, Key, WindowEvent};
use gouache::*;
use gouache::renderers::GlRenderer;

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
    song: Song,
    cursor: (usize, usize),
    playing: bool,
}

impl Default for Editor {
    fn default() -> Editor {
        Editor {
            song: Song::default(),
            cursor: (0, 0),
            playing: false,
        }
    }
}

struct Context {
    cursor: Vec2,
    modifiers: glfw::Modifiers,
    mouse_captured: bool,
}

struct Rect {
    pos: Vec2,
    size: Vec2,
}

impl Rect {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect { pos: Vec2::new(x, y), size: Vec2::new(width, height) }
    }

    fn contains(&self, point: Vec2) -> bool {
        point.x >= self.pos.x && point.x < self.pos.x + self.size.x &&
        point.y >= self.pos.y && point.y < self.pos.y + self.size.y
    }
}

fn main() {
    let mut window = Window::new(800, 600, "justitracker");

    let mut cache = Cache::new();
    let mut renderer = GlRenderer::new();

    let font = Font::from_bytes(include_bytes!("../res/SourceSansPro-Regular.ttf")).unwrap();

    let mut audio = Audio::start().unwrap();

    let mut editor = Editor::default();

    let play_icon = PathBuilder::new()
        .move_to(4.0, 3.0)
        .line_to(4.0, 13.0)
        .line_to(12.0, 8.0)
        .build();
    let mut play = Button {
        rect: Rect::new(0.0, 0.0, 16.0, 16.0),
        icon: play_icon,
        down: false,
    };

    let (cell_w, cell_h) = font.measure("00", 14.0);
    let (cell_w, cell_h) = (cell_w.ceil(), cell_h.ceil());
    let cell_spacing = 2.0;

    let mut context = Context {
        cursor: Vec2::new(-1.0, -1.0),
        modifiers: glfw::Modifiers::empty(),
        mouse_captured: false,
    };

    let mut running = true;
    while running && !window.should_close() {
        let mut frame = Frame::new(&mut cache, &mut renderer, 800.0, 600.0);
        frame.clear(Color::rgba(0.1, 0.15, 0.2, 1.0));

        let toolbar_height = 24.0;

        play.render(&mut frame, &context);

        let offset = Vec2::new(0.0, toolbar_height);
        for t in 0..editor.song.tracks {
            for r in 0..editor.song.length {
                let offset = offset + Vec2::new(4.0 * t as f32 * (cell_w + cell_spacing), r as f32 * (cell_h + cell_spacing));

                if editor.cursor == (t, r) {
                    frame.draw_rect(
                        offset,
                        Vec2::new(4.0 * cell_w + 3.0 * cell_spacing, cell_h),
                        Mat2x2::id(),
                        Color::rgba(0.141, 0.44, 0.77, 1.0),
                    );
                }

                let note = editor.song.notes[t * editor.song.tracks + r];
                for f in 0..4 {
                    let text = match note {
                        Note::On(value) => format!("{:02}", value[f]),
                        Note::Off => "--".to_string(),
                        Note::None => ". .".to_string(),
                    };
                    frame.draw_text(
                        &font,
                        14.0,
                        &text,
                        offset + Vec2::new(f as f32 * (cell_w + cell_spacing), 0.0),
                        Mat2x2::id(),
                        Color::rgba(1.0, 1.0, 1.0, 1.0),
                    );
                }
            }
        }

        frame.finish();

        window.swap();

        window.poll(|event| {
            match event {
                WindowEvent::Close => { running = false; }
                WindowEvent::Key(Key::Escape, _, Action::Press, _) => { running = false; }
                WindowEvent::Key(key, _, action, modifiers) => {
                    if action == Action::Press || action == Action::Repeat {
                        match key {
                            Key::Left => { editor.cursor.0 = editor.cursor.0.saturating_sub(1) }
                            Key::Right => { editor.cursor.0 = (editor.cursor.0 + 1).min(editor.song.tracks - 1) }
                            Key::Up => { editor.cursor.1 = editor.cursor.1.saturating_sub(1) }
                            Key::Down => { editor.cursor.1 = (editor.cursor.1 + 1).min(editor.song.length - 1) }
                            Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 => {
                                let mut note = &mut editor.song.notes[editor.cursor.0 * editor.song.length + editor.cursor.1];
                                let mut value = if let Note::On(value) = note { *value } else { [0; 4] };
                                let inc = if modifiers.contains(glfw::Modifiers::Shift) { -1 } else { 1 };
                                let idx = match key {
                                    Key::Num1 => { value[0] += inc }
                                    Key::Num2 => { value[1] += inc }
                                    Key::Num3 => { value[2] += inc }
                                    Key::Num4 => { value[3] += inc }
                                    _ => {}
                                };
                                *note = Note::On(value);
                                audio.send(Msg::Song(editor.song.clone()));
                            }
                            Key::GraveAccent => {
                                editor.song.notes[editor.cursor.0 * editor.song.length + editor.cursor.1] = Note::Off;
                                audio.send(Msg::Song(editor.song.clone()));
                            }
                            Key::Backspace | Key::Delete => {
                                editor.song.notes[editor.cursor.0 * editor.song.length + editor.cursor.1] = Note::None;
                                audio.send(Msg::Song(editor.song.clone()));
                            }
                            Key::I => {
                                if let Ok(nfd::Response::Okay(path)) = nfd::open_file_dialog(Some("wav"), None) {
                                    if let Ok(mut reader) = hound::WavReader::open(path) {
                                        editor.song.samples[editor.cursor.0] = match reader.spec().sample_format {
                                            hound::SampleFormat::Float => {
                                                reader.samples::<f32>().map(|s| s.unwrap() as f32).collect()
                                            }
                                            hound::SampleFormat::Int => {
                                                reader.samples::<i32>().map(|s| s.unwrap() as f32 / 32768.0).collect()
                                            }
                                        };
                                        audio.send(Msg::Song(editor.song.clone()));
                                    }
                                }
                            }
                            Key::Space => {
                                if editor.playing {
                                    editor.playing = false;
                                    audio.send(Msg::Stop);
                                } else {
                                    editor.playing = true;
                                    audio.send(Msg::Play);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::CursorPos(x, y) => {
                    context.cursor = Vec2::new(x as f32, y as f32);
                }
                WindowEvent::MouseButton(..) => {
                    if play.handle(event, &mut context) {
                        editor.playing = true;
                        audio.send(Msg::Play);
                    }
                }
                _ => {}
            }
        });
    }
}

struct Button {
    rect: Rect,
    icon: Path,
    down: bool,
}

impl Button {
    fn render(&self, frame: &mut Frame, context: &Context) {
        let color = if self.down {
            Color::rgba(0.141, 0.44, 0.77, 1.0)
        } else if self.rect.contains(context.cursor) {
            Color::rgba(0.54, 0.63, 0.71, 1.0)
        } else {
            Color::rgba(0.38, 0.42, 0.48, 1.0)
        };

        frame.draw_rect(self.rect.pos, self.rect.size, Mat2x2::id(), color);
        frame.draw_path(&self.icon, self.rect.pos, Mat2x2::id(), Color::rgba(1.0, 1.0, 1.0, 1.0));
    }

    fn handle(&mut self, input: glfw::WindowEvent, context: &mut Context) -> bool {
        match input {
            WindowEvent::MouseButton(glfw::MouseButton::Button1, glfw::Action::Press, _) => {
                if !context.mouse_captured && self.rect.contains(context.cursor) {
                    self.down = true;
                    context.mouse_captured = true;
                }
            }
            WindowEvent::MouseButton(glfw::MouseButton::Button1, glfw::Action::Release, _) => {
                if self.down {
                    context.mouse_captured = false;
                    self.down = false;
                    if self.rect.contains(context.cursor) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }
}
