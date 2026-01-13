use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
    pub file_path: PathBuf,
    pub cover_art: Option<String>,
    pub is_webdav: bool,
    pub webdav_url: Option<String>,
}

impl Song {
    pub fn new(file_path: PathBuf, is_webdav: bool, webdav_url: Option<String>) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let title = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        
        Song {
            id,
            title,
            artist: String::from("Unknown Artist"),
            album: String::from("Unknown Album"),
            duration: 0,
            file_path,
            cover_art: None,
            is_webdav,
            webdav_url,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub songs: Vec<Song>,
    pub created_at: u64,
}

impl Playlist {
    pub fn new(name: String) -> Self {
        Playlist {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            songs: Vec::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn add_song(&mut self, song: Song) {
        if !self.songs.iter().any(|s| s.id == song.id) {
            self.songs.push(song);
        }
    }

    pub fn remove_song(&mut self, song_id: &str) {
        self.songs.retain(|s| s.id != song_id);
    }

    pub fn clear(&mut self) {
        self.songs.clear();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WebDAVConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
}

impl WebDAVConfig {
    pub fn new(name: String, url: String, username: String, password: String) -> Self {
        WebDAVConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            url,
            username,
            password,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebDAVFile {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
pub enum PlayerCommand {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Seek(u32),
    SetVolume(f32),
    PlaySong(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub current_song: Option<Song>,
    pub playback_state: PlaybackState,
    pub current_position: u32,
    pub volume: f32,
    pub current_playlist_index: Option<usize>,
}
