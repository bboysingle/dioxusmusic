use rodio::{Decoder, OutputStream, Sink, Source};
use std::fs::File;
use std::io::{BufReader, Cursor, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod lyrics;
pub use lyrics::{Lyric, LyricLine};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerState {
    Playing,
    Paused,
    Stopped,
}

const MAX_FILE_SIZE: u64 = 200 * 1024 * 1024; // 200MB limit for streaming

#[derive(Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub path: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Clone, Default)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub cover: Option<Vec<u8>>,
    pub duration: Duration,
    pub lyrics: Option<String>,
}

impl TrackMetadata {
    pub fn from_path(path: &Path) -> Self {
        use id3::{Tag, TagLike};
        use metaflac::Tag as FlacTag;

        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let mut metadata = TrackMetadata::default();

        // Try ID3 tags first (MP3, M4A)
        if let Ok(tag) = Tag::read_from_path(path).or_else(|_| {
            let file = File::open(path)?;
            Tag::read_from(file)
        }) {
            metadata.title = tag.title().map(|t| t.to_string()).or(Some(file_name.clone()));
            metadata.artist = tag.artist().map(|a| a.to_string());
            metadata.album = tag.album().map(|a| a.to_string());
            metadata.cover = tag.pictures().next().map(|pic| pic.data.clone());

            // Read lyrics from USLT frame
            if let Some(lyrics) = tag.lyrics().next() {
                metadata.lyrics = Some(lyrics.text.to_string());
            }
        }

        // Try FLAC tags
        if metadata.title.is_none() || metadata.artist.is_none() {
            if let Ok(tag) = FlacTag::read_from_path(path) {
                if let Some(vorbis) = tag.vorbis_comments() {
                    if metadata.title.is_none() {
                        metadata.title = vorbis.title()
                            .and_then(|v| v.first().cloned())
                            .or(Some(file_name.clone()));
                    }
                    if metadata.artist.is_none() {
                        metadata.artist = vorbis.artist().and_then(|v| v.first().cloned());
                    }
                    if metadata.album.is_none() {
                        metadata.album = vorbis.album().and_then(|v| v.first().cloned());
                    }
                }
                if metadata.cover.is_none() {
                    metadata.cover = tag.pictures().next().map(|pic| pic.data.clone());
                }
            }
        }

        // Get duration
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            if let Ok(source) = Decoder::new(reader) {
                metadata.duration = source.total_duration().unwrap_or(Duration::from_secs(0));
            }
        }

        if metadata.title.is_none() {
            metadata.title = Some(file_name);
        }

        metadata
    }
}

pub struct MusicPlayer {
    sink: Arc<Mutex<Option<Sink>>>,
    _stream: OutputStream,
    current_duration: Arc<Mutex<Duration>>,
    current_time: Arc<Mutex<Duration>>,
    current_path: Arc<Mutex<Option<PathBuf>>>,
    on_track_end: Arc<Mutex<Option<Box<dyn FnMut() + Send + 'static>>>>,
    temp_file: Arc<Mutex<Option<PathBuf>>>,
    playlist: Arc<Mutex<Vec<Track>>>,
    current_index: Arc<Mutex<usize>>,
    auto_play: Arc<Mutex<bool>>,
    last_track_path: Arc<Mutex<Option<String>>>,
    last_track_id: Arc<Mutex<Option<String>>>,
    pub track_ended: Arc<Mutex<bool>>,
    last_elapsed: Arc<Mutex<std::time::Instant>>,
    pub stopped_by_user: Arc<Mutex<bool>>,
    is_playing: Arc<Mutex<bool>>,
    current_metadata: Arc<Mutex<Option<TrackMetadata>>>,
    downloaded_bytes: Arc<Mutex<u64>>,
    total_bytes: Arc<Mutex<u64>>,
    is_remote: Arc<Mutex<bool>>,
    current_lyric: Arc<Mutex<Option<Lyric>>>,
}

impl Clone for MusicPlayer {
    fn clone(&self) -> Self {
        MusicPlayer {
            sink: Arc::clone(&self.sink),
            _stream: OutputStream::try_default().unwrap_or_else(|_| panic!("Failed to create output stream")).0,
            current_duration: Arc::clone(&self.current_duration),
            current_time: Arc::clone(&self.current_time),
            current_path: Arc::clone(&self.current_path),
            on_track_end: Arc::clone(&self.on_track_end),
            temp_file: Arc::clone(&self.temp_file),
            playlist: Arc::clone(&self.playlist),
            current_index: Arc::clone(&self.current_index),
            auto_play: Arc::clone(&self.auto_play),
            last_track_path: Arc::clone(&self.last_track_path),
            last_track_id: Arc::clone(&self.last_track_id),
            track_ended: Arc::clone(&self.track_ended),
            last_elapsed: Arc::clone(&self.last_elapsed),
            stopped_by_user: Arc::clone(&self.stopped_by_user),
            is_playing: Arc::clone(&self.is_playing),
            current_metadata: Arc::clone(&self.current_metadata),
            downloaded_bytes: Arc::clone(&self.downloaded_bytes),
            total_bytes: Arc::clone(&self.total_bytes),
            is_remote: Arc::clone(&self.is_remote),
            current_lyric: Arc::clone(&self.current_lyric),
        }
    }
}

impl MusicPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (_stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        Ok(MusicPlayer {
            sink: Arc::new(Mutex::new(Some(sink))),
            _stream,
            current_duration: Arc::new(Mutex::new(Duration::from_secs(0))),
            current_time: Arc::new(Mutex::new(Duration::from_secs(0))),
            current_path: Arc::new(Mutex::new(None)),
            on_track_end: Arc::new(Mutex::new(None)),
            temp_file: Arc::new(Mutex::new(None)),
            playlist: Arc::new(Mutex::new(Vec::new())),
            current_index: Arc::new(Mutex::new(0)),
            auto_play: Arc::new(Mutex::new(true)),
            last_track_path: Arc::new(Mutex::new(None)),
            last_track_id: Arc::new(Mutex::new(None)),
            track_ended: Arc::new(Mutex::new(false)),
            last_elapsed: Arc::new(Mutex::new(std::time::Instant::now())),
            stopped_by_user: Arc::new(Mutex::new(false)),
            is_playing: Arc::new(Mutex::new(false)),
            current_metadata: Arc::new(Mutex::new(None)),
            downloaded_bytes: Arc::new(Mutex::new(0)),
            total_bytes: Arc::new(Mutex::new(0)),
            is_remote: Arc::new(Mutex::new(false)),
            current_lyric: Arc::new(Mutex::new(None)),
        })
    }

    pub fn play(&self, path: &Path, track_id: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
        *self.is_playing.lock().unwrap() = true;

        if let Some(id) = track_id {
            if let Ok(mut guard) = self.last_track_id.lock() {
                *guard = Some(id);
            }
        }

        let path_str = path.to_string_lossy();
        let extension = path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        let (source, duration) = if path_str.starts_with("http://") || path_str.starts_with("https://") {
            let source = self.play_remote_url(&path_str)?;
            let duration = source.total_duration().unwrap_or(Duration::from_secs(0));
            (Box::new(source) as Box<dyn rodio::Source<Item = i16> + Send>, duration)
        } else {
            let source = self.play_local_file(path, &extension)?;
            let duration = source.total_duration().unwrap_or(Duration::from_secs(0));
            (source, duration)
        };

        // Set duration
        self.set_duration(duration);

        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.stop();
                sink.append(source);
                sink.play();
            } else {
                return Err("音频输出设备不可用".into());
            }
        } else {
            return Err("无法访问音频输出设备".into());
        }

        if let Ok(mut path_guard) = self.current_path.lock() {
            *path_guard = Some(path.to_path_buf());
        }

        let on_track_end = self.on_track_end.clone();
        let last_track_id_clone = self.last_track_id.clone();
        let track_ended_clone = self.track_ended.clone();
        let weak_sink = Arc::downgrade(&self.sink);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Some(sink_arc) = weak_sink.upgrade() {
                    if let Ok(sink_guard) = sink_arc.lock() {
                        if let Some(sink) = sink_guard.as_ref() {
                            if sink.empty() {
                                if let Ok(mut callback_guard) = on_track_end.lock() {
                                    if let Some(callback) = callback_guard.as_mut() {
                                        callback();
                                    }
                                }
                                // Mark track as ended
                                *track_ended_clone.lock().unwrap() = true;
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        });
        
        eprintln!("[Player] 曲目播放结束 (自然结束)");
        
        Ok(())
    }

    fn play_local_file(&self, path: &Path, extension: &str) -> Result<Box<dyn rodio::Source<Item = i16> + Send>, Box<dyn std::error::Error>> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("无法访问文件 '{}': {}", path.display(), e))?;

        if !metadata.is_file() {
            return Err(format!("'{}' 不是一个文件", path.display()).into());
        }

        if metadata.len() == 0 {
            return Err(format!("文件 '{}' 为空", path.display()).into());
        }

        let file = File::open(path)
            .map_err(|e| format!("无法打开文件 '{}': {}", path.display(), e))?;

        let file_size = file.metadata()?.len();

        if file_size > MAX_FILE_SIZE {
            return Err(format!("文件过大 ({}MB)，当前不支持播放超过 {}MB 的音频文件",
                              file_size / (1024 * 1024), MAX_FILE_SIZE / (1024 * 1024)).into());
        }

        let buf_reader = BufReader::new(file);
        
        match std::panic::catch_unwind(|| {
            Decoder::new(buf_reader)
        }) {
            Ok(Ok(source)) => Ok(Box::new(source) as Box<dyn rodio::Source<Item = i16> + Send>),
            Ok(Err(rodio_error)) => {
                Err(format!("音频解码失败 '{}': {}. 文件大小: {} bytes, 扩展名: {}",
                          path.display(), rodio_error, file_size, extension).into())
            }
            Err(_) => {
                Err(format!("音频解码器在处理文件 '{}' 时发生内部错误。文件大小: {} bytes, 扩展名: {}",
                          path.display(), file_size, extension).into())
            }
        }
    }

    fn play_remote_url(&self, url: &str) -> Result<Box<dyn rodio::Source<Item = i16> + Send>, Box<dyn std::error::Error>> {
        eprintln!("[Player] 从URL下载音频: {}", url);

        let url = url.to_string();
        let temp_dir = std::env::temp_dir();
        let temp_filename = format!("dioxus_music_{}", uuid::Uuid::new_v4());
        let temp_path = temp_dir.join(&temp_filename);

        let (tx, rx) = std::sync::mpsc::channel();
        let progress_sender = Arc::new(std::sync::Mutex::new(0u64));
        let total_sender = Arc::new(std::sync::Mutex::new(0u64));
        let player_progress = self.downloaded_bytes.clone();
        let player_total = self.total_bytes.clone();

        let progress_clone = progress_sender.clone();
        let total_clone = total_sender.clone();
        let _ = std::thread::spawn(move || {
            let result = std::fs::write(&temp_path, vec![]);
            if result.is_err() {
                let _ = tx.send(Err(format!("无法创建临时文件: {:?}", result)));
                return;
            }

            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build();

            if let Err(e) = client {
                let _ = tx.send(Err(format!("创建HTTP客户端失败: {}", e)));
                return;
            }

            let client = client.unwrap();
            let response = client.get(&url).send();

            if let Err(e) = response {
                let _ = tx.send(Err(format!("无法下载音频文件: {}", e)));
                return;
            }

            let response = response.unwrap();

            if !response.status().is_success() {
                let _ = tx.send(Err(format!("下载失败 (HTTP {})", response.status())));
                return;
            }

            let content_length = response.content_length().unwrap_or(0);
            *total_clone.lock().unwrap() = content_length;

            if content_length > MAX_FILE_SIZE {
                let _ = tx.send(Err(format!("文件过大 ({}MB)，当前不支持播放超过 {}MB 的音频文件",
                    content_length / (1024 * 1024), MAX_FILE_SIZE / (1024 * 1024))));
                return;
            }

            let mut downloaded = 0;
            {
                let mut file = std::fs::File::create(&temp_path).map_err(|e| {
                    let _ = tx.send(Err(format!("无法创建临时文件: {}", e)));
                    return;
                }).unwrap();

                let bytes = response.bytes().map_err(|e| {
                    let _ = tx.send(Err(format!("读取音频数据失败: {}", e)));
                    return;
                }).unwrap();

                let bytes_vec = bytes.to_vec();

                if let Err(e) = file.write_all(&bytes_vec) {
                    let _ = tx.send(Err(format!("写入文件失败: {}", e)));
                    return;
                }
                downloaded = bytes_vec.len() as u64;
                *progress_clone.lock().unwrap() = downloaded;
                *player_progress.lock().unwrap() = downloaded;
                *player_total.lock().unwrap() = content_length;
            }

            if downloaded == 0 {
                let _ = tx.send(Err("音频文件为空".to_string()));
                return;
            }

            let _ = tx.send(Ok(temp_path));
        });

        let temp_path = rx.recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|e| format!("下载超时: {}", e))?
            .map_err(|e| e)?;

        *self.downloaded_bytes.lock().unwrap() = 0;
        *self.total_bytes.lock().unwrap() = 0;

        let file = File::open(&temp_path)
            .map_err(|e| format!("无法打开临时文件: {}", e))?;

        let buf_reader = BufReader::new(file);

        match std::panic::catch_unwind(|| {
            Decoder::new(buf_reader)
        }) {
            Ok(Ok(source)) => {
                let metadata = TrackMetadata::from_path(&temp_path);
                eprintln!("[Player] 提取到元数据: title={:?}, artist={:?}, album={:?}, duration={:?}",
                    metadata.title, metadata.artist, metadata.album, metadata.duration);
                self.update_metadata(metadata);

                if let Ok(mut temp_guard) = self.temp_file.lock() {
                    if let Some(old_temp) = temp_guard.take() {
                        let _ = std::fs::remove_file(&old_temp);
                    }
                    *temp_guard = Some(temp_path.clone());
                }
                Ok(Box::new(source) as Box<dyn rodio::Source<Item = i16> + Send>)
            }
            Ok(Err(rodio_error)) => {
                let _ = std::fs::remove_file(&temp_path);
                Err(format!("音频解码失败: {}. 文件大小: {} bytes", rodio_error, std::fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0)).into())
            }
            Err(_) => {
                let _ = std::fs::remove_file(&temp_path);
                Err("音频解码器发生内部错误".into())
            }
        }
    }

    pub fn cleanup_temp_file(&self) {
        if let Ok(mut temp_guard) = self.temp_file.lock() {
            if let Some(temp_path) = temp_guard.take() {
                let _ = std::fs::remove_file(&temp_path);
                eprintln!("[Player] 已清理临时文件: {:?}", temp_path);
            }
        }
    }

    pub fn pause(&self) {
        *self.is_playing.lock().unwrap() = false;
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.pause();
            }
        }
    }

    pub fn resume(&self) {
        *self.is_playing.lock().unwrap() = true;
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.play();
            }
        }
    }

    pub fn stop(&self) {
        *self.is_playing.lock().unwrap() = false;
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
    
    pub fn is_finished(&self) -> bool {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                return sink.empty();
            }
        }
        false
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
        }
    }

    pub fn get_last_track_id(&self) -> Option<String> {
        self.last_track_id.lock().unwrap().clone()
    }
    
    pub fn reset_track_ended(&self) {
        *self.track_ended.lock().unwrap() = false;
    }
    
    pub fn get_current_time(&self) -> Duration {
        *self.current_time.lock().unwrap()
    }
    
    pub fn get_duration(&self) -> Duration {
        *self.current_duration.lock().unwrap()
    }
    
    pub fn get_elapsed(&self) -> Duration {
        let is_playing = *self.is_playing.lock().unwrap();
        if is_playing {
            let total = self.get_duration();
            let now = std::time::Instant::now();
            let last = *self.last_elapsed.lock().unwrap();
            let diff = now.duration_since(last);
            let elapsed = diff.min(total);
            *self.current_time.lock().unwrap() = elapsed;
            return elapsed;
        }
        *self.current_time.lock().unwrap()
    }

    pub fn get_current_metadata(&self) -> Option<TrackMetadata> {
        self.current_metadata.lock().unwrap().clone()
    }

    pub fn get_download_progress(&self) -> f64 {
        let downloaded = *self.downloaded_bytes.lock().unwrap();
        let total = *self.total_bytes.lock().unwrap();
        if total > 0 {
            (downloaded as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn is_remote_track(&self) -> bool {
        *self.is_remote.lock().unwrap()
    }

    pub fn update_download_progress(&self, downloaded: u64, total: u64) {
        *self.downloaded_bytes.lock().unwrap() = downloaded;
        *self.total_bytes.lock().unwrap() = total;
    }

    pub fn set_remote(&self, is_remote: bool) {
        *self.is_remote.lock().unwrap() = is_remote;
        if !is_remote {
            *self.downloaded_bytes.lock().unwrap() = 0;
            *self.total_bytes.lock().unwrap() = 0;
        }
    }

    pub fn update_metadata(&self, metadata: TrackMetadata) {
        *self.current_metadata.lock().unwrap() = Some(metadata.clone());
        eprintln!("[Player] 已更新元数据: {:?}", metadata.title);
    }
    
    pub fn set_duration(&self, duration: Duration) {
        *self.current_duration.lock().unwrap() = duration;
        *self.last_elapsed.lock().unwrap() = std::time::Instant::now();
    }
    
    pub fn set_stopped_by_user(&self, stopped: bool) {
        *self.stopped_by_user.lock().unwrap() = stopped;
    }
    
    pub fn seek(&self, time: Duration) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(sink_guard) = self.sink.lock() {
            if let Some(sink) = sink_guard.as_ref() {
                sink.stop();

                // Check for temporary file first (WebDAV downloads)
                let temp_path = {
                    let temp_guard = self.temp_file.lock().unwrap();
                    temp_guard.clone()
                };

                let play_path = if let Some(temp) = temp_path {
                    eprintln!("[Player] Using temp file for seek: {:?}", temp);
                    temp
                } else {
                    let path_guard = self.current_path.lock().unwrap();
                    if let Some(path) = path_guard.clone() {
                        path
                    } else {
                        *self.current_time.lock().unwrap() = time;
                        return Ok(());
                    }
                };

                let path_str = play_path.to_string_lossy();
                let is_remote = path_str.contains("dioxus_music_");

                if is_remote {
                    eprintln!("[Player] Seeking remote track to {} seconds", time.as_secs());
                } else {
                    eprintln!("[Player] Seeking to {} seconds", time.as_secs());
                }

                let path_clone = play_path.clone();
                let extension = path_clone.extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let source = self.play_local_file_with_seek(&path_clone, &extension, time)?;

                sink.append(source);
                sink.play();

                // Set last_elapsed so that get_elapsed() returns the seek time
                *self.last_elapsed.lock().unwrap() = std::time::Instant::now() - time;
                *self.current_time.lock().unwrap() = time;

                return Ok(());
            }
        }
        Err("Failed to seek".into())
    }
    
    fn play_local_file_with_seek(&self, path: &Path, extension: &str, seek_time: Duration) -> Result<Box<dyn rodio::Source<Item = i16> + Send>, Box<dyn std::error::Error>> {
        match extension {
            "mp3" => {
                let file = std::fs::File::open(path)?;
                let metadata = std::fs::metadata(path)?;
                let file_size = metadata.len();
                
                // Estimate byte position: assume ~128kbps average bitrate
                let bytes_per_second = 16000;
                let seek_byte = (seek_time.as_secs() * bytes_per_second).min(file_size.saturating_sub(100));
                
                let mut file = BufReader::new(file);
                
                if seek_byte > 0 {
                    let _ = file.seek(SeekFrom::Start(seek_byte));
                    eprintln!("[Player] MP3 seeked to byte {}", seek_byte);
                }
                
                match Decoder::new(file) {
                    Ok(source) => Ok(Box::new(source) as Box<dyn rodio::Source<Item = i16> + Send>),
                    Err(e) => Err(format!("Failed to decode MP3: {}", e).into()),
                }
            }
            "wav" => {
                let data = std::fs::read(path)?;
                let data_len = data.len();
                let mut cursor = Cursor::new(data);
                
                // WAV header is 44 bytes, each sample is 4 bytes (16-bit stereo)
                let bytes_per_sample = 4;
                let sample_rate = 44100;
                let bytes_to_skip = 44 + (seek_time.as_secs() as u64 * sample_rate as u64 * bytes_per_sample as u64);
                
                if bytes_to_skip < data_len as u64 && bytes_to_skip > 44 {
                    if cursor.seek(SeekFrom::Start(bytes_to_skip)).is_ok() {
                        eprintln!("[Player] WAV seeked to position {} seconds", seek_time.as_secs());
                    }
                }
                
                match Decoder::new_wav(cursor) {
                    Ok(source) => Ok(Box::new(source) as Box<dyn rodio::Source<Item = i16> + Send>),
                    Err(e) => Err(format!("Failed to decode WAV: {}", e).into()),
                }
            }
            "flac" => {
                // FLAC seeking is complex, just restart from beginning for now
                eprintln!("[Player] FLAC seek not fully implemented, restarting from beginning");
                self.play_local_file(path, extension)
            }
            _ => {
                self.play_local_file(path, extension)
            }
        }
    }

    pub fn get_lyric(&self) -> Option<Lyric> {
        let guard = self.current_lyric.lock().unwrap();
        guard.clone()
    }

    pub fn set_lyric(&self, lyric: Option<Lyric>) {
        let mut guard = self.current_lyric.lock().unwrap();
        *guard = lyric;
    }

    pub fn load_local_lyric(&self, music_path: &std::path::Path) {
        if let Some(lyric_path) = lyrics::find_local_lyric(music_path) {
            eprintln!("[Player] 找到本地歌词文件: {:?}", lyric_path);
            match lyrics::load_local_lyric(&lyric_path) {
                Ok(lyric) if !lyric.is_empty() => {
                    self.set_lyric(Some(lyric));
                    eprintln!("[Player] 本地歌词加载成功");
                }
                _ => {
                    eprintln!("[Player] 本地歌词解析失败");
                }
            }
        }
    }

    pub async fn fetch_lyrics_for_current_track(&self, title: &str, artist: &str) {
        if title.is_empty() {
            return;
        }

        eprintln!("[Player] Fetching lyrics for: {} - {}", artist, title);

        let embedded_lyrics = {
            let guard = self.current_metadata.lock().unwrap();
            guard.as_ref().and_then(|m| m.lyrics.clone())
        };

        let music_path = {
            let guard = self.current_path.lock().unwrap();
            guard.clone()
        };

        match lyrics::fetch_lyrics_for_track(title, artist, embedded_lyrics.as_deref(), music_path.as_deref()).await {
            Ok(lyric) => {
                if !lyric.is_empty() {
                    self.set_lyric(Some(lyric));
                    eprintln!("[Player] Lyrics loaded successfully");
                } else {
                    eprintln!("[Player] No lyrics found");
                }
            }
            Err(e) => {
                eprintln!("[Player] Failed to fetch lyrics: {}", e);
            }
        }
    }
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self::new().expect("Failed to initialize music player")
    }
}