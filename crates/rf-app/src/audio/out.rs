//! [audio14] Audio output. Only this file touches `rodio`; everything else talks to the
//! [`Backend`] trait. [`NullBackend`] records calls (tests, muted runs); [`RodioBackend`]
//! (feature `audio`) streams the Ogg Vorbis files from the DF install.

use std::path::{Path, PathBuf};

pub type VoiceId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Music,
    Ambience,
    Sfx,
    Ui,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayOpts {
    pub looped: bool,
    pub gain: f32,
    /// -1 left .. 1 right (one-shots only).
    pub pan: f32,
    /// Muffle (one-shots only): low-pass cut-off in Hz.
    pub low_pass: Option<u32>,
}

pub trait Backend {
    fn play(&mut self, bus: Bus, path: &Path, opts: PlayOpts) -> Option<VoiceId>;
    fn set_gain(&mut self, v: VoiceId, gain: f32);
    fn pause(&mut self, v: VoiceId, paused: bool);
    fn stop(&mut self, v: VoiceId);
    /// The voice ended (or never existed).
    fn finished(&self, v: VoiceId) -> bool;
    /// Per-frame upkeep (refill loops, drop ended voices).
    fn tick(&mut self) {}
    /// Voices currently alive on a bus.
    fn voices(&self, bus: Bus) -> usize;
}

/// Records what would play; nothing is ever audible.
#[derive(Debug, Default)]
pub struct NullBackend {
    next: VoiceId,
    pub log: Vec<(Bus, PathBuf, PlayOpts)>,
    pub alive: Vec<(VoiceId, Bus)>,
}

impl Backend for NullBackend {
    fn play(&mut self, bus: Bus, path: &Path, opts: PlayOpts) -> Option<VoiceId> {
        self.next += 1;
        self.log.push((bus, path.to_path_buf(), opts));
        if self.log.len() > 256 {
            self.log.drain(..128);
        }
        if opts.looped || bus == Bus::Music {
            self.alive.push((self.next, bus));
        }
        Some(self.next)
    }
    fn set_gain(&mut self, _: VoiceId, _: f32) {}
    fn pause(&mut self, _: VoiceId, _: bool) {}
    fn stop(&mut self, v: VoiceId) {
        self.alive.retain(|a| a.0 != v);
    }
    /// One-shots end at once; music and loops play until stopped.
    fn finished(&self, v: VoiceId) -> bool {
        !self.alive.iter().any(|a| a.0 == v)
    }
    fn voices(&self, bus: Bus) -> usize {
        self.alive.iter().filter(|a| a.1 == bus).count()
    }
}

#[cfg(feature = "audio")]
pub use rodio_backend::RodioBackend;

#[cfg(feature = "audio")]
mod rodio_backend {
    use super::*;
    use rodio::source::Buffered;
    use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::BufReader;

    type Dec = Decoder<BufReader<File>>;

    /// Most voices per bus (music 2 for fades).
    fn cap(bus: Bus) -> usize {
        match bus {
            Bus::Music => 2,
            Bus::Ambience => 6,
            Bus::Sfx => 8,
            Bus::Ui => 2,
        }
    }

    const SFX_CACHE: usize = 32;

    struct Voice {
        id: VoiceId,
        bus: Bus,
        sink: Sink,
        /// Looping: the file to queue again.
        looped: Option<PathBuf>,
    }

    pub struct RodioBackend {
        _stream: OutputStream,
        handle: OutputStreamHandle,
        voices: Vec<Voice>,
        next: VoiceId,
        cache: BTreeMap<PathBuf, Buffered<Dec>>,
        cache_order: Vec<PathBuf>,
    }

    fn open(p: &Path) -> Option<Dec> {
        Decoder::new(BufReader::new(File::open(p).ok()?)).ok()
    }

    impl RodioBackend {
        /// Open the default output device. Must stay on the main thread.
        pub fn new() -> Result<RodioBackend, String> {
            let (stream, handle) = OutputStream::try_default().map_err(|e| e.to_string())?;
            Ok(RodioBackend { _stream: stream, handle, voices: Vec::new(), next: 0, cache: BTreeMap::new(), cache_order: Vec::new() })
        }

        fn buffered(&mut self, p: &Path) -> Option<Buffered<Dec>> {
            if let Some(b) = self.cache.get(p) {
                return Some(b.clone());
            }
            let b = open(p)?.buffered();
            if self.cache_order.len() >= SFX_CACHE {
                let old = self.cache_order.remove(0);
                self.cache.remove(&old);
            }
            self.cache_order.push(p.to_path_buf());
            self.cache.insert(p.to_path_buf(), b.clone());
            Some(b)
        }
    }

    impl Backend for RodioBackend {
        fn play(&mut self, bus: Bus, path: &Path, o: PlayOpts) -> Option<VoiceId> {
            self.voices.retain(|v| !v.sink.empty());
            if self.voices.iter().filter(|v| v.bus == bus).count() >= cap(bus) {
                // Drop the oldest one-shot of the bus; loops and music are managed by the caller.
                let i = self.voices.iter().position(|v| v.bus == bus && v.looped.is_none())?;
                self.voices.remove(i).sink.stop();
            }
            let sink = Sink::try_new(&self.handle).ok()?;
            sink.set_volume(o.gain.max(0.0));
            if o.looped {
                // Streamed and queued twice; `tick` queues it again as each copy ends.
                sink.append(open(path)?);
                sink.append(open(path)?);
            } else if bus == Bus::Music {
                sink.append(open(path)?);
            } else {
                let src = self.buffered(path)?.convert_samples::<f32>();
                let (l, r) = ((1.0 - o.pan.clamp(-1.0, 1.0)).min(1.0), (1.0 + o.pan.clamp(-1.0, 1.0)).min(1.0));
                match o.low_pass {
                    Some(hz) => sink.append(rodio::source::ChannelVolume::new(src.low_pass(hz), vec![l, r])),
                    None => sink.append(rodio::source::ChannelVolume::new(src, vec![l, r])),
                }
            }
            self.next += 1;
            self.voices.push(Voice { id: self.next, bus, sink, looped: o.looped.then(|| path.to_path_buf()) });
            Some(self.next)
        }
        fn set_gain(&mut self, v: VoiceId, gain: f32) {
            if let Some(x) = self.voices.iter().find(|x| x.id == v) {
                x.sink.set_volume(gain.max(0.0));
            }
        }
        fn pause(&mut self, v: VoiceId, paused: bool) {
            if let Some(x) = self.voices.iter().find(|x| x.id == v) {
                if paused {
                    x.sink.pause();
                } else {
                    x.sink.play();
                }
            }
        }
        fn stop(&mut self, v: VoiceId) {
            if let Some(i) = self.voices.iter().position(|x| x.id == v) {
                self.voices.remove(i).sink.stop();
            }
        }
        fn finished(&self, v: VoiceId) -> bool {
            self.voices.iter().find(|x| x.id == v).map(|x| x.sink.empty()).unwrap_or(true)
        }
        fn tick(&mut self) {
            for v in &self.voices {
                if let Some(p) = &v.looped {
                    if v.sink.len() < 2 {
                        if let Some(d) = open(p) {
                            v.sink.append(d);
                        }
                    }
                }
            }
        }
        fn voices(&self, bus: Bus) -> usize {
            self.voices.iter().filter(|v| v.bus == bus && !v.sink.empty()).count()
        }
    }
}

/// Decode the start of a file without any output device (tests, diagnostics): sample rate,
/// channels and how many samples were read (at most `max`).
#[cfg(feature = "audio")]
pub fn probe(path: &Path, max: usize) -> Result<(u32, u16, usize), String> {
    use rodio::Source;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let d = rodio::Decoder::new(std::io::BufReader::new(f)).map_err(|e| e.to_string())?;
    let (rate, ch) = (d.sample_rate(), d.channels());
    Ok((rate, ch, d.take(max).count()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_backend_tracks_voices() {
        let mut b = NullBackend::default();
        let m = b.play(Bus::Music, Path::new("a.ogg"), PlayOpts::default()).unwrap();
        let s = b.play(Bus::Sfx, Path::new("b.ogg"), PlayOpts::default()).unwrap();
        assert!(!b.finished(m) && b.finished(s));
        assert_eq!(b.voices(Bus::Music), 1);
        b.stop(m);
        assert!(b.finished(m));
    }

    /// Decoding smoke test against the real install (skipped without one; never plays).
    #[cfg(feature = "audio")]
    #[test]
    fn decodes_df_vorbis_without_a_device() {
        let (Some(df), _) = rf_raws::df_dir_status() else {
            eprintln!("skipped: no DF install");
            return;
        };
        let p = df.join("data").join("sound").join("sounds").join("alert.ogg");
        if !p.is_file() {
            eprintln!("skipped: {} missing", p.display());
            return;
        }
        let (rate, ch, n) = probe(&p, 4096).unwrap();
        assert!(rate == 48_000 || rate == 44_100, "{rate}");
        assert!((1..=2).contains(&ch));
        assert_eq!(n, 4096);
    }
}
