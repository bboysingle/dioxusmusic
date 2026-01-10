# Code Examples & Usage Guide

## Table of Contents
1. [Basic Playback](#basic-playback)
2. [Playlist Management](#playlist-management)
3. [Metadata Extraction](#metadata-extraction)
4. [WebDAV Integration](#webdav-integration)
5. [UI Components](#ui-components)

## Basic Playback

### Simple Play/Pause Example

```rust
use std::path::Path;
use player::MusicPlayer;

// Initialize player
let player = MusicPlayer::new()?;

// Play a track
player.play(Path::new("music/song.mp3"))?;

// Pause playback
player.pause();

// Resume playback
player.resume();

// Stop playback
player.stop();

// Set volume (0.0 to 1.0)
player.set_volume(0.5); // 50% volume
```

### Volume Control

```rust
// Increase volume
let current_volume = use_signal(|| 0.5);
*current_volume.write() = (*current_volume.read() + 0.1).clamp(0.0, 1.0);

// In RSX
input {
    r#type: "range",
    min: "0",
    max: "100",
    value: (current_volume() * 100.0) as i32,
    oninput: move |e| {
        let vol = e.value().parse::<f32>().unwrap_or(1.0) / 100.0;
        *current_volume.write() = vol;
        player.set_volume(vol);
    }
}
```

## Playlist Management

### Create and Manage Playlists

```rust
use playlist::Playlist;
use std::time::Duration;

// Create a new playlist
let mut my_playlist = Playlist::new("My Favorite Songs".to_string());

// Add tracks to playlist
let track = Track {
    id: uuid::Uuid::new_v4().to_string(),
    path: "/music/song.mp3".to_string(),
    title: "Song Title".to_string(),
    artist: "Artist Name".to_string(),
    album: "Album Name".to_string(),
    duration: Duration::from_secs(180),
    cover: None,
};

my_playlist.add_track(track);

// Remove a track by ID
my_playlist.remove_track("track-id-here");

// Clear all tracks
my_playlist.clear();

// Get track count
let count = my_playlist.tracks.len();
```

### Save and Load Playlists

```rust
// Save a single playlist to file
my_playlist.save_to_file("playlists/my_playlist.json")?;

// Load a playlist from file
let loaded_playlist = Playlist::load_from_file("playlists/my_playlist.json")?;

// Save multiple playlists
let playlists = vec![playlist1, playlist2, playlist3];
for playlist in &playlists {
    let filename = format!("playlists/{}.json", playlist.id);
    playlist.save_to_file(&filename)?;
}

// Load all playlists from directory
let all_playlists = Playlist::load_multiple_from_dir("playlists/")?;
```

### Batch Operations

```rust
// Scan directory and create playlist
pub fn load_music_to_playlist(dir: &str) -> Result<Playlist, Box<dyn std::error::Error>> {
    let mut playlist = Playlist::new("Music Library".to_string());
    
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ["mp3", "wav", "flac", "ogg", "m4a"].contains(&ext.to_lowercase().as_str()) {
                if let Ok(track) = TrackMetadata::from_file(path) {
                    playlist.add_track(track);
                }
            }
        }
    }
    
    Ok(playlist)
}
```

## Metadata Extraction

### Extract Metadata from Files

```rust
use metadata::TrackMetadata;
use std::path::Path;

// Extract from single file
let track = TrackMetadata::from_file(Path::new("music/song.mp3"))?;
println!("Title: {}", track.title);
println!("Artist: {}", track.artist);
println!("Album: {}", track.album);
println!("Duration: {}s", track.duration.as_secs());

// Extract from multiple files
let music_files = vec![
    "music/song1.mp3",
    "music/song2.flac",
    "music/song3.wav",
];

let tracks: Result<Vec<_>, _> = music_files
    .iter()
    .map(|f| TrackMetadata::from_file(Path::new(f)))
    .collect();

match tracks {
    Ok(track_list) => println!("Loaded {} tracks", track_list.len()),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Handle Missing Metadata

```rust
// Graceful fallback
let track = match TrackMetadata::from_file(path) {
    Ok(t) => t,
    Err(_) => {
        // Create track with filename as title
        Track {
            id: uuid::Uuid::new_v4().to_string(),
            path: path.to_string_lossy().to_string(),
            title: path.file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string(),
            artist: "Unknown Artist".to_string(),
            album: "Unknown Album".to_string(),
            duration: Duration::from_secs(0),
            cover: None,
        }
    }
};
```

## WebDAV Integration

### Basic WebDAV Operations

```rust
use webdav::WebDAVClient;

// Create client
let client = WebDAVClient::new("https://nextcloud.example.com/webdav/".to_string());

// Add authentication
let authenticated_client = client
    .with_auth("username".to_string(), "password".to_string());

// List files
let files = authenticated_client.list_files("/music").await?;
for file in files {
    println!("Found: {}", file);
}

// Download a file
authenticated_client
    .download_file("/music/song.mp3", "./downloads/song.mp3")
    .await?;

// Upload a file
authenticated_client
    .upload_file("./local/song.mp3", "/music/uploaded.mp3")
    .await?;
```

### Create Playlist from WebDAV

```rust
pub async fn create_webdav_playlist(
    url: &str,
    username: &str,
    password: &str,
    path: &str,
) -> Result<Playlist, Box<dyn std::error::Error>> {
    let client = WebDAVClient::new(url.to_string())
        .with_auth(username.to_string(), password.to_string());
    
    let mut playlist = Playlist::new("Cloud Music".to_string());
    let files = client.list_files(path).await?;
    
    for file_path in files {
        if file_path.ends_with(".mp3") || file_path.ends_with(".flac") {
            // Download and extract metadata
            let local_file = format!("/tmp/{}", 
                file_path.split('/').last().unwrap_or("track.mp3"));
            
            client.download_file(&file_path, &local_file).await?;
            
            if let Ok(track) = TrackMetadata::from_file(Path::new(&local_file)) {
                playlist.add_track(track);
            }
            
            // Clean up
            std::fs::remove_file(&local_file)?;
        }
    }
    
    Ok(playlist)
}
```

## UI Components

### Creating a Track Selection Component

```rust
#[component]
fn TrackSelector(
    tracks: Vec<Track>,
    on_select: EventHandler<Track>,
) -> Element {
    rsx! {
        div {
            class: "track-list",
            for track in tracks.iter().enumerate() {
                let (idx, track) = track;
                let track_clone = track.clone();
                
                button {
                    key: "{idx}",
                    onclick: move |_| on_select.call(track_clone.clone()),
                    "{track.title} - {track.artist}"
                }
            }
        }
    }
}
```

### Building a Now Playing Display

```rust
#[component]
fn NowPlaying(track: Option<Track>) -> Element {
    rsx! {
        div {
            class: "now-playing",
            if let Some(t) = track {
                div {
                    h2 { "{t.title}" }
                    p { "{t.artist}" }
                    p { "{t.album}" }
                    p { "{format_duration(t.duration)}" }
                }
            } else {
                p { "No track selected" }
            }
        }
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}
```

### Volume Control Component

```rust
#[component]
fn VolumeControl(
    volume: f32,
    on_change: EventHandler<f32>,
) -> Element {
    rsx! {
        div {
            class: "volume-control",
            label { "🔊 Volume" }
            input {
                r#type: "range",
                min: "0",
                max: "100",
                value: (volume * 100.0) as i32,
                oninput: move |e| {
                    let vol = e.value().parse::<f32>().unwrap_or(1.0) / 100.0;
                    on_change.call(vol);
                }
            }
            span { "{(volume * 100.0) as i32}%" }
        }
    }
}
```

### Progress Bar with Seek

```rust
#[component]
fn ProgressBar(
    current: Duration,
    total: Option<Duration>,
    on_seek: EventHandler<Duration>,
) -> Element {
    let percent = if let Some(d) = total {
        if d.as_secs() > 0 {
            (current.as_secs_f64() / d.as_secs_f64() * 100.0) as i32
        } else {
            0
        }
    } else {
        0
    };

    rsx! {
        div {
            class: "progress-bar",
            div {
                class: "progress-container",
                div {
                    class: "progress-fill",
                    style: "width: {percent}%"
                }
            }
            div {
                class: "progress-time",
                span { "{format_duration(current)}" }
                span { "{format_duration(total.unwrap_or(Duration::from_secs(0)))}" }
            }
        }
    }
}
```

## Advanced Patterns

### Using Coroutines for Background Tasks

```rust
#[component]
fn PlayerWithProgress() -> Element {
    let mut current_time = use_signal(|| Duration::from_secs(0));
    
    let _progress_task = use_coroutine(move |_rx| async move {
        loop {
            // Update progress every 100ms
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            *current_time.write() = Duration::from_secs(100); // Get from player
        }
    });
    
    rsx! {
        // Use current_time for display
    }
}
```

### Context for Global Player State

```rust
#[derive(Clone)]
pub struct PlayerContext {
    pub player: Arc<MusicPlayer>,
    pub current_track: Signal<Option<Track>>,
    pub is_playing: Signal<bool>,
}

#[component]
fn App() -> Element {
    let player = MusicPlayer::new().unwrap();
    let current_track = use_signal(|| None);
    let is_playing = use_signal(|| false);
    
    let context = PlayerContext {
        player: Arc::new(player),
        current_track,
        is_playing,
    };
    
    use_context_provider(|| context);
    
    rsx! {
        PlayerInterface {}
    }
}

#[component]
fn PlayerInterface() -> Element {
    let context = use_context::<PlayerContext>();
    
    rsx! {
        button {
            onclick: move |_| {
                // Use context.player, context.current_track, etc.
            },
            "Play"
        }
    }
}
```

### Error Handling in Components

```rust
#[component]
fn PlaylistLoader(path: String) -> Element {
    let mut playlists = use_signal(|| vec![]);
    let mut error = use_signal(|| None::<String>);
    
    use_effect(move || {
        let path = path.clone();
        async move {
            match Playlist::load_multiple_from_dir(&path) {
                Ok(loaded) => *playlists.write() = loaded,
                Err(e) => *error.write() = Some(e.to_string()),
            }
        }
    });
    
    rsx! {
        if let Some(err) = error() {
            div {
                class: "error",
                "Error loading playlists: {err}"
            }
        } else {
            div {
                class: "playlists",
                for playlist in playlists() {
                    div { "{playlist.name}" }
                }
            }
        }
    }
}
```

## Testing Examples

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playlist_creation() {
        let playlist = Playlist::new("Test".to_string());
        assert_eq!(playlist.name, "Test");
        assert!(playlist.tracks.is_empty());
    }

    #[test]
    fn test_add_track() {
        let mut playlist = Playlist::new("Test".to_string());
        let track = Track {
            id: "1".to_string(),
            path: "test.mp3".to_string(),
            title: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            album: "Test Album".to_string(),
            duration: Duration::from_secs(180),
            cover: None,
        };
        
        playlist.add_track(track);
        assert_eq!(playlist.tracks.len(), 1);
    }

    #[test]
    fn test_remove_track() {
        let mut playlist = Playlist::new("Test".to_string());
        let track = Track {
            id: "1".to_string(),
            // ... other fields
        };
        
        playlist.add_track(track);
        playlist.remove_track("1");
        assert!(playlist.tracks.is_empty());
    }
}
```

These examples cover the main usage patterns for the music player application.
