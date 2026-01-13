use crate::models::{Playlist, Song, WebDAVConfig};
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

const PLAYLISTS_DIR: &str = "playlists";
const CONFIG_FILE: &str = "config.json";

pub struct PlaylistManager {
    playlists: Vec<Playlist>,
    webdav_configs: Vec<WebDAVConfig>,
    base_dir: PathBuf,
}

impl PlaylistManager {
    pub fn new() -> Result<Self> {
        let base_dir = Self::get_data_dir()?;
        let playlists_dir = base_dir.join(PLAYLISTS_DIR);
        
        fs::create_dir_all(&playlists_dir)?;
        
        let mut manager = PlaylistManager {
            playlists: Vec::new(),
            webdav_configs: Vec::new(),
            base_dir,
        };
        
        manager.load_playlists()?;
        manager.load_config()?;
        
        Ok(manager)
    }

    fn get_data_dir() -> Result<PathBuf> {
        let home_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        
        let app_dir = home_dir.join(".dioxusmusic");
        fs::create_dir_all(&app_dir)?;
        Ok(app_dir)
    }

    pub fn create_playlist(&mut self, name: String) -> Playlist {
        let playlist = Playlist::new(name);
        self.playlists.push(playlist.clone());
        playlist
    }

    pub fn get_playlists(&self) -> &[Playlist] {
        &self.playlists
    }

    pub fn get_playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn get_playlist_mut(&mut self, id: &str) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    pub fn delete_playlist(&mut self, id: &str) -> Result<()> {
        if let Some(index) = self.playlists.iter().position(|p| p.id == id) {
            self.playlists.remove(index);
            let playlist_file = self.base_dir.join(PLAYLISTS_DIR).join(format!("{}.json", id));
            if playlist_file.exists() {
                fs::remove_file(playlist_file)?;
            }
        }
        Ok(())
    }

    pub fn add_songs_to_playlist(&mut self, playlist_id: &str, songs: Vec<Song>) -> Result<()> {
        if let Some(playlist) = self.get_playlist_mut(playlist_id) {
            for song in songs {
                playlist.add_song(song);
            }
            self.save_playlist(playlist_id)?;
        }
        Ok(())
    }

    pub fn remove_song_from_playlist(&mut self, playlist_id: &str, song_id: &str) -> Result<()> {
        if let Some(playlist) = self.get_playlist_mut(playlist_id) {
            playlist.remove_song(song_id);
            self.save_playlist(playlist_id)?;
        }
        Ok(())
    }

    fn save_playlist(&self, playlist_id: &str) -> Result<()> {
        if let Some(playlist) = self.get_playlist(playlist_id) {
            let playlist_file = self.base_dir.join(PLAYLISTS_DIR).join(format!("{}.json", playlist_id));
            let json = serde_json::to_string_pretty(playlist)?;
            fs::write(playlist_file, json)?;
        }
        Ok(())
    }

    fn load_playlists(&mut self) -> Result<()> {
        let playlists_dir = self.base_dir.join(PLAYLISTS_DIR);
        
        if !playlists_dir.exists() {
            return Ok(());
        }
        
        for entry in fs::read_dir(playlists_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(playlist) = serde_json::from_str::<Playlist>(&content) {
                    self.playlists.push(playlist);
                }
            }
        }
        
        Ok(())
    }

    pub fn save_config(&self) -> Result<()> {
        let config = Config {
            webdav_configs: self.webdav_configs.clone(),
        };
        
        let config_file = self.base_dir.join(CONFIG_FILE);
        let json = serde_json::to_string_pretty(&config)?;
        fs::write(config_file, json)?;
        
        Ok(())
    }

    fn load_config(&mut self) -> Result<()> {
        let config_file = self.base_dir.join(CONFIG_FILE);
        
        if !config_file.exists() {
            return Ok(());
        }
        
        let content = fs::read_to_string(&config_file)?;
        if let Ok(config) = serde_json::from_str::<Config>(&content) {
            self.webdav_configs = config.webdav_configs;
        }
        
        Ok(())
    }

    pub fn add_webdav_config(&mut self, config: WebDAVConfig) -> Result<()> {
        if !self.webdav_configs.iter().any(|c| c.id == config.id) {
            self.webdav_configs.push(config.clone());
            self.save_config()?;
        }
        Ok(())
    }

    pub fn remove_webdav_config(&mut self, id: &str) -> Result<()> {
        if self.webdav_configs.iter().any(|c| c.id == id) {
            self.webdav_configs.retain(|c| c.id != id);
            self.save_config()?;
        }
        Ok(())
    }

    pub fn get_webdav_configs(&self) -> &[WebDAVConfig] {
        &self.webdav_configs
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Config {
    webdav_configs: Vec<WebDAVConfig>,
}

impl Default for PlaylistManager {
    fn default() -> Self {
        Self::new().expect("Failed to create PlaylistManager")
    }
}
