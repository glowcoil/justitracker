use crate::{Song, Note};

use portaudio as pa;

const SAMPLE_RATE: f64 = 44_100.0;
const FRAMES: u32 = 256;
const CHANNELS: i32 = 2;

pub enum Msg {
    Play,
    Stop,
    Song(Song),
}

pub struct Audio {
    portaudio: pa::PortAudio,
    stream: pa::Stream<pa::NonBlocking, pa::Output<f32>>,
    tx: std::sync::mpsc::Sender<Msg>,
}

impl Audio {
    pub fn start() -> Result<Audio, pa::Error> {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut song = Song::default();
        let mut playing = false;
        let mut t: usize = 0;
        let mut note: usize = 0;

        let bpm = 120.0;
        let note_length = ((60.0 / bpm as f32) * SAMPLE_RATE as f32).round() as usize;

        let portaudio = pa::PortAudio::new()?;
        let settings = portaudio.default_output_stream_settings(CHANNELS, SAMPLE_RATE, FRAMES)?;
        let mut stream = portaudio.open_non_blocking_stream(settings, move |args| {
            for msg in rx.try_iter() {
                match msg {
                    Msg::Play => { playing = true; t = 0; note = 0; }
                    Msg::Stop => { playing = false; }
                    Msg::Song(new_song) => { song = new_song; }
                }
            }

            if playing {
                for sample in args.buffer.iter_mut() {
                    t += 1;
                    if t == note_length {
                        t = 0;
                        note = (note + 1) % song.length;
                    }

                    let mut mix: f32 = 0.0;
                    for (track, sample) in song.notes.chunks(song.tracks).zip(song.samples.iter()) {
                        let mut previous = note;
                        let mut length = 0;
                        while let Note::None = track[previous] {
                            previous = previous.saturating_sub(1);
                            length += 1;
                            if previous == 0 { break; }
                        }
                        if let Note::On(ref factors) = track[previous] {
                            let pitch = 2.0f32.powi(factors[0]) * (3.0f32 / 2.0f32).powi(factors[1]) * (5.0f32 / 4.0f32).powi(factors[2]) * (7.0f32 / 4.0f32).powi(factors[3]);
                            let phase: f32 = ((length * note_length + t) as f32 * pitch) % sample.len() as f32;

                            let phase_whole = phase as usize;
                            let phase_frac = phase - phase_whole as f32;
                            let value = (1.0 - phase_frac) * sample[phase_whole] + phase_frac * sample[(phase_whole + 1) % sample.len()];

                            mix += value;
                        }
                    }
                    *sample = mix.max(-1.0).min(1.0);
                }
            } else {
                for sample in args.buffer.iter_mut() {
                    *sample = 0.0;
                }
            }

            pa::Continue
        })?;

        stream.start()?;

        Ok(Audio { portaudio, stream, tx })
    }

    pub fn send(&self, msg: Msg) {
        self.tx.send(msg);
    }
}
