use std::thread;
use std::sync::mpsc;
use std::f32;
use std::f32::consts;

use cpal;
use cpal::{EventLoop, UnknownTypeBuffer};

use {Song, Note};

pub enum AudioMessage {
    Play,
    Stop,
    Song(Song),
}

pub struct Engine {
    sample_rate: u32,
    song: Song,
    t: u32,
    note: usize,
}

impl Engine {
    pub fn new(sample_rate: u32, song: Song) -> Engine {
        Engine {
            sample_rate: sample_rate,
            song: song,
            t: 0,
            note: 0,
        }
    }

    pub fn song(&mut self, song: Song) {
        self.song = song;
        self.note %= self.song.ptn_len;
    }

    pub fn reset(&mut self) {
        self.t = 0;
        self.note = 0;
    }

    pub fn calculate(&mut self, buffer: &mut [f32]) {
        let note_length = ((60.0 / self.song.bpm as f32) * self.sample_rate as f32) as u32;
        for elem in buffer.iter_mut() {
            self.t += 1;
            let mut mix: f32 = 0.0;
            for track in 0..self.song.notes.len() {
                if self.t == note_length {
                    self.t = 0;
                    self.note = (self.note + 1) % self.song.ptn_len;
                }

                let mut previous = self.note;
                let mut length = 0;
                while let Note::None = self.song.notes[track][previous] {
                    previous = previous.saturating_sub(1);
                    length += 1;
                    if previous == 0 { break; }
                }
                if let Note::On(ref factors) = self.song.notes[track][previous] {
                    let pitch = 2.0f32.powi(factors[0]) * (3.0f32 / 2.0f32).powi(factors[1]) * (5.0f32 / 4.0f32).powi(factors[2]) * (7.0f32 / 4.0f32).powi(factors[3]);
                    let phase: f32 = ((length * note_length + self.t) as f32 * pitch) % self.song.samples[track].len() as f32;

                    let phase_whole = phase as usize;
                    let phase_frac = phase - phase_whole as f32;
                    let value = (1.0 - phase_frac) * self.song.samples[track][phase_whole] + phase_frac * self.song.samples[track][(phase_whole + 1) % self.song.samples[track].len()];

                    mix += value;
                }
            }
            if mix > 1.0 {
                mix = 1.0;
            } else if mix < -1.0 {
                mix = -1.0;
            }
            *elem = mix;
        }
    }
}

pub fn start_audio_thread() -> (u32, mpsc::Sender<AudioMessage>) {
    let (send, recv) = mpsc::channel();

    let endpoint = cpal::default_endpoint().expect("no output device available");
    let mut supported_formats_range = endpoint.supported_formats()
        .expect("error while querying formats");
    let format = supported_formats_range.next()
        .expect("no supported format?!")
        .with_max_samples_rate();

    let sample_rate = format.samples_rate.0;

    thread::spawn(move || {
        let event_loop = EventLoop::new();
        let voice_id = event_loop.build_voice(&endpoint, &format).unwrap();
        event_loop.play(voice_id);

        let mut playing = false;

        let mut engine: Engine = Engine::new(sample_rate, Song::default());

        event_loop.run(move |_voice_id, buffer| {
            for msg in recv.try_iter() {
                match msg {
                    AudioMessage::Play => {
                        playing = true;
                    }
                    AudioMessage::Stop => {
                        playing = false;
                        engine.reset();
                    }
                    AudioMessage::Song(song) => {
                        engine.song(song);
                    }
                }
            }

            match buffer {
                UnknownTypeBuffer::U16(mut buffer) => {
                    for elem in buffer.iter_mut() {
                        *elem = u16::max_value() / 2;
                    }
                },
                UnknownTypeBuffer::I16(mut buffer) => {
                    for elem in buffer.iter_mut() {
                        *elem = 0;
                    }
                },
                UnknownTypeBuffer::F32(mut buffer) => {
                    if playing {
                        engine.calculate(&mut *buffer);
                    } else {
                        for elem in buffer.iter_mut() {
                            *elem = 0.0;
                        }
                    }
                },
            }
        });
    });

    (sample_rate, send)
}
