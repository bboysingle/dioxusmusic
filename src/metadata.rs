use crate::Track;
use id3::{Tag, TagLike};
use metaflac::Tag as FlacTag;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;
use rodio::Source;

pub fn extract_metadata(path: &Path) -> Result<Track, Box<dyn std::error::Error>> {
    let path_str = path.to_string_lossy().to_string();
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    // Try to get duration
    let duration = get_duration(path)?;

    // Try ID3 tags first (MP3)
    if let Ok(tag) = Tag::read_from_path(path) {
        let title = tag.title()
            .map(|t| t.to_string())
            .unwrap_or_else(|| file_name.clone());
        
        let artist = tag.artist()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        
        let album = tag.album()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "Unknown Album".to_string());

        // Try to extract cover art
        let cover = tag.pictures()
            .next()
            .map(|pic| pic.data.clone());

        return Ok(Track {
            id: Uuid::new_v4().to_string(),
            path: path_str,
            title,
            artist,
            album,
            duration,
            cover,
        });
    }

    // Try FLAC tags
    if let Ok(tag) = FlacTag::read_from_path(path) {
        if let Some(vorbis) = tag.vorbis_comments() {
            let title = vorbis.title()
                .and_then(|v| v.first().cloned())
                .unwrap_or_else(|| file_name.clone());
            
            let artist = vorbis.artist()
                .and_then(|v| v.first().cloned())
                .unwrap_or_else(|| "Unknown Artist".to_string());
            
            let album = vorbis.album()
                .and_then(|v| v.first().cloned())
                .unwrap_or_else(|| "Unknown Album".to_string());

            // FLAC pictures
            let cover = tag.pictures()
                .next()
                .map(|pic| pic.data.clone());

            return Ok(Track {
                id: Uuid::new_v4().to_string(),
                path: path_str,
                title,
                artist,
                album,
                duration,
                cover,
            });
        }
    }

    // Fallback to filename
    Ok(Track {
        id: Uuid::new_v4().to_string(),
        path: path_str,
        title: file_name,
        artist: "Unknown Artist".to_string(),
        album: "Unknown Album".to_string(),
        duration,
        cover: None,
    })
}

fn get_duration(path: &Path) -> Result<Duration, Box<dyn std::error::Error>> {
    use rodio::Decoder;
    use std::fs::File;
    use std::io::BufReader;

    let file = BufReader::new(File::open(path)?);
    let source = Decoder::new(file)?;
    Ok(source.total_duration().unwrap_or(Duration::from_secs(0)))
}

pub struct TrackMetadata;

impl TrackMetadata {
    pub fn from_file(path: &Path) -> Result<Track, Box<dyn std::error::Error>> {
        extract_metadata(path)
    }
}
