use std::thread;
use std::sync::mpsc;
use std::f32;
use std::f32::consts;

use cpal;
use cpal::{EventLoop, UnknownTypeBuffer};

pub enum AudioMessage {
    Play,
    Stop,
    Song([f32; 8]),
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
        println!("{:?}", format.samples_rate.0);
        let voice_id = event_loop.build_voice(&endpoint, &format).unwrap();
        event_loop.play(voice_id);

        let mut playing = false;

        let bpm: f32 = 120.0;
        let mut t: f32 = 0.0;
        let mut note: usize = 0;

        let mut phase: f32 = 0.0;
        let mut sin: [f32; 128] = [0.0; 128];
        for (i, x) in sin.iter_mut().enumerate() {
            *x = (i as f32 * 2.0 * consts::PI / 128.0).sin();
        }

        let root = 55.0 * sin.len() as f32 / format.samples_rate.0 as f32;
        let mut song: [f32; 8] = [0.0; 8];

        event_loop.run(move |_voice_id, buffer| {
            for msg in recv.try_iter() {
                match msg {
                    AudioMessage::Play => {
                        playing = true;
                    }
                    AudioMessage::Stop => {
                        playing = false;
                        t = 0.0;
                        phase = 0.0;
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
                    for elem in buffer.iter_mut() {
                        if playing {
                            t += 1.0 / format.samples_rate.0 as f32;
                            let note_length = 60.0 / bpm;
                            if t > note_length {
                                t -= note_length;
                                phase = 0.0;
                                note = (note + 1) % 8;
                            }

                            let pitch = song[note] * root;
                            phase = (phase + pitch) % sin.len() as f32;

                            let phase_whole = phase as usize;
                            let phase_frac = phase - phase_whole as f32;
                            let value = (1.0 - phase_frac) * sin[phase_whole] + phase_frac * sin[(phase_whole + 1) % sin.len()];
                            *elem = value;
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