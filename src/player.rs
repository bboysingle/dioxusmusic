use rodio::{Decoder, OutputStream, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerState {
    Playing,
    Paused,
    Stopped,
}

pub struct MusicPlayer {
    sink: Arc<Mutex<Option<Sink>>>,
    _stream: OutputStream,
    current_duration: Arc<Mutex<Duration>>,
    current_path: Arc<Mutex<Option<PathBuf>>>,
}

impl MusicPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (_stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        Ok(MusicPlayer {
            sink: Arc::new(Mutex::new(Some(sink))),
            _stream,
            current_duration: Arc::new(Mutex::new(Duration::from_secs(0))),
            current_path: Arc::new(Mutex::new(None)),
        })
    }

    pub fn play(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let file = BufReader::new(File::open(path)?);
        let source = Decoder::new(file)?;

        if let Ok(mut sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.stop();
                sink.append(source);
                sink.play();
            }
        }

        // 记录当前路径
        if let Ok(mut path_guard) = self.current_path.lock() {
            *path_guard = Some(path.to_path_buf());
        }

        Ok(())
    }

    pub fn pause(&self) {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.pause();
            }
        }
    }

    pub fn resume(&self) {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.play();
            }
        }
    }

    pub fn stop(&self) {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.stop();
            }
        }
        if let Ok(mut path_guard) = self.current_path.lock() {
            *path_guard = None;
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.set_volume(volume.clamp(0.0, 1.0));
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                return sink.is_paused();
            }
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                return sink.empty();
            }
        }
        true
    }

    pub fn get_current_path(&self) -> Option<PathBuf> {
        if let Ok(path_guard) = self.current_path.lock() {
            path_guard.clone()
        } else {
            None
        }    }
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self::new().expect("Failed to initialize music player")
    }
}