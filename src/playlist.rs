use crate::Track;
use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn new(name: String) -> Self {
        Playlist {
            id: Uuid::new_v4().to_string(),
            name,
            tracks: Vec::new(),
        }
    }

    pub fn add_track(&mut self, track: Track) {
        self.tracks.push(track);
    }

    pub fn remove_track(&mut self, track_id: &str) {
        self.tracks.retain(|t| t.id != track_id);
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let playlist = serde_json::from_str(&json)?;
        Ok(playlist)
    }

    pub fn load_multiple_from_dir(dir_path: &str) -> Result<Vec<Self>, Box<dyn std::error::Error>> {
        let mut playlists = Vec::new();
        
        if !std::path::Path::new(dir_path).exists() {
            fs::create_dir_all(dir_path)?;
        }

        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(playlist) = Self::load_from_file(path.to_str().unwrap_or("")) {
                    playlists.push(playlist);
                }
            }
        }

        Ok(playlists)
    }
}
