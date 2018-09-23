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

pub fn start_audio_thread() -> mpsc::Sender<AudioMessage> {
    let (send, recv) = mpsc::channel();

    thread::spawn(move || {
        let event_loop = EventLoop::new();
        let endpoint = cpal::default_endpoint().expect("no output device available");
        let mut supported_formats_range = endpoint.supported_formats()
            .expect("error while querying formats");
        let format = supported_formats_range.next()
            .expect("no supported format?!")
            .with_max_samples_rate();
        let voice_id = event_loop.build_voice(&endpoint, &format).unwrap();
        event_loop.play(voice_id);

        let mut playing = false;

        let mut t: u32 = 0;
        let mut note: usize = 0;

        let mut sin: [f32; 128] = [0.0; 128];
        for (i, x) in sin.iter_mut().enumerate() {
            *x = (i as f32 * 2.0 * consts::PI / 128.0).sin();
        }

        let mut song: Song = Song::default();

        event_loop.run(move |_voice_id, buffer| {
            for msg in recv.try_iter() {
                match msg {
                    AudioMessage::Play => {
                        playing = true;
                    }
                    AudioMessage::Stop => {
                        playing = false;
                        t = 0;
                        note = 0;
                    }
                    AudioMessage::Song(s) => {
                        song = s;
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
                    let note_length = ((60.0 / song.bpm as f32) * format.samples_rate.0 as f32) as u32;
                    for elem in buffer.iter_mut() {
                        if playing {
                            t += 1;
                            let mut mix: f32 = 0.0;
                            for track in 0..song.notes.len() {
                                if t == note_length {
                                    t = 0;
                                    note = (note + 1) % song.ptn_len;
                                }

                                let mut previous = note;
                                let mut length = 0;
                                while let Note::None = song.notes[track][previous] {
                                    previous = previous.saturating_sub(1);
                                    length += 1;
                                    if previous == 0 { break; }
                                }
                                if let Note::On(ref factors) = song.notes[track][previous] {
                                    let pitch = 2.0f32.powi(factors[0]) * 3.0f32.powi(factors[1]) * 5.0f32.powi(factors[2]) * 7.0f32.powi(factors[3]);
                                    let phase: f32 = ((length * note_length + t) as f32 * pitch) % song.samples[track].len() as f32;

                                    let phase_whole = phase as usize;
                                    let phase_frac = phase - phase_whole as f32;
                                    let value = (1.0 - phase_frac) * song.samples[track][phase_whole] + phase_frac * song.samples[track][(phase_whole + 1) % song.samples[track].len()];

                                    mix += value;
                                }
                            }
                            if mix > 1.0 {
                                mix = 1.0;
                            } else if mix < -1.0 {
                                mix = -1.0;
                            }
                            *elem = mix;
                        } else {
                            *elem = 0.0;
                        }
                    }
                },
            }
        });
    });

    send
}