# dioxusmusic

A full-featured music player built with Dioxus 0.7 and Rust. Play local music files, manage playlists, and access cloud music via WebDAV.

## Features

### ✅  Features

- [x] **Play local music files** - Support for MP3, WAV, FLAC, OGG, M4A formats using Rodio
- [x] **Control music playback** - Play, pause, stop, and seek controls
- [x] **Volume control** - Adjustable volume slider (0-100%)
- [x] **Display current track information** - Shows title, artist, album, and duration
- [x] **Album cover display** - Extract and display cover art from ID3 and FLAC tags
- [x] **Create and manage playlists** - Create multiple playlists with drag-and-drop UI
- [x] **Save and load playlists** - Persist playlists to JSON files in `playlists/` directory
- [x] **Track metadata extraction** - Extract metadata from MP3 (ID3v2) and FLAC tags
- [x] **WebDAV support** - Browse, download, and upload music from WebDAV servers (Nextcloud, Aliyun, etc.)
- [x] **Music library scanning** - Recursively scan directories for audio files
- [x] **Progress bar** - Visual progress indicator with current/total time display
- [x] **Track selection UI** - Click to play any track in the playlist

### 🎨 UI Components

- Dark modern theme using Tailwind CSS
- Responsive layout with sidebar playlists and main player area
- Now playing card with track info
- Playlist tracks browser with scroll
- Modal for creating new playlists
- Volume and progress controls

## Project Structure

```
project/
├─ src/
│  ├─ main.rs           # Main app and UI components
│  ├─ player.rs         # Rodio music player wrapper
│  ├─ playlist.rs       # Playlist management and persistence
│  ├─ metadata.rs       # ID3/FLAC metadata extraction
│  └─ webdav.rs         # WebDAV client for cloud music
├─ assets/
│  ├─ main.css
│  └─ tailwind.css
├─ Cargo.toml           # Dependencies and features
├─ Dioxus.toml          # Dioxus configuration
└─ playlists/           # Saved playlists (auto-created)
```

## Dependencies

- **dioxus** - UI framework (0.7.1)
- **rodio** - Audio playback (0.18)
- **id3** - MP3 metadata (1.16)
- **metaflac** - FLAC metadata (0.2)
- **walkdir** - Directory traversal (2)
- **serde/serde_json** - Serialization
- **tokio** - Async runtime
- **reqwest** - HTTP client for WebDAV
- **uuid** - Unique identifiers
- **base64** - Image encoding

## Getting Started

### Prerequisites
- Rust 1.70+
- Dioxus CLI

### Installation

```bash
# Install Dioxus CLI
curl -sSL http://dioxus.dev/install.sh | sh

# Navigate to project
cd dioxusmusic

# Run the app
dx serve
```

The app will start at `http://localhost:8080` in web mode, or as a desktop app in desktop mode.

## Usage

### Loading Music Files

1. Use the scan feature to load music from a directory:
```rust
let tracks = scan_music_directory("/path/to/music")?;
```

2. Add tracks to a playlist:
```rust
let mut playlist = Playlist::new("My Music".to_string());
for track in tracks {
    playlist.add_track(track);
}
```

### Playing Music

1. Click on a track in the playlist to select it
2. Click the **▶ Play** button or press play
3. Use **⏸ Pause** to pause playback
4. Use **⏹ Stop** to stop playback
5. Drag the progress bar to seek to a specific time
6. Adjust the volume slider to control volume

### Managing Playlists

1. Click **+ New** to create a new playlist
2. Enter a playlist name and click **Create**
3. Click on tracks to play them
4. To save playlists:
```rust
save_all_playlists(&playlists, "playlists/")?;
```

5. To load saved playlists:
```rust
let playlists = load_all_playlists("playlists/")?;
```

### WebDAV Cloud Music

Connect to cloud storage services:

```rust
let client = WebDAVClient::new("https://nextcloud.example.com/remote.php/dav/files/username/music/".to_string())
    .with_auth("username".to_string(), "password".to_string());

// List files
let files = client.list_files("/").await?;

// Download a file
client.download_file("/song.mp3", "./music/song.mp3").await?;

// Upload a file
client.upload_file("./music/new_song.mp3", "/new_song.mp3").await?;
```

Supported cloud services:
- Nextcloud
- Aliyun OSS (with WebDAV gateway)
- Any WebDAV-compatible service

## Architecture

### Player Module (`player.rs`)
- `MusicPlayer` struct wraps Rodio sink
- Handles audio playback state management
- Provides pause, resume, stop, and volume control

### Playlist Module (`playlist.rs`)
- `Playlist` struct for track collections
- JSON persistence with UUID-based file naming
- Load/save individual or multiple playlists

### Metadata Module (`metadata.rs`)
- `TrackMetadata` for extracting tag information
- Supports ID3v2 (MP3) and Vorbis (FLAC) comments
- Extracts cover art and duration

### WebDAV Module (`webdav.rs`)
- `WebDAVClient` for cloud music access
- Basic auth support
- PROPFIND for directory listing
- GET/PUT for file operations

## Building for Different Targets

### Web
```bash
dx build --release
```

### Desktop
```bash
dx build --release --features desktop
```

### Mobile (iOS/Android)
```bash
dx build --release --features mobile
```

## Configuration

Edit `Dioxus.toml` to customize:
- App title and icon
- Window size and properties
- Build output directory

## Advanced Features

### Custom Playlist Saving
```rust
let playlist = Playlist::new("My Playlist".to_string());
// ... add tracks ...
playlist.save_to_file("custom_playlist.json")?;
```

### Metadata Extraction
```rust
use metadata::TrackMetadata;
let track = TrackMetadata::from_file(Path::new("song.mp3"))?;
println!("Title: {}", track.title);
println!("Artist: {}", track.artist);
```

### Volume Control
```rust
player.set_volume(0.5); // 50% volume
```

## Performance Tips

- **Lazy loading**: Scan directories only when needed
- **Caching**: Metadata is cached after extraction
- **Streaming**: Rodio streams audio to avoid memory issues with large files
- **Async I/O**: WebDAV operations are async to prevent UI blocking

## Future Enhancements

- [ ] Shuffle and repeat modes
- [ ] Queue management
- [ ] Search and filter tracks
- [ ] Equalizer controls
- [ ] Lyrics display
- [ ] Last.fm integration
- [ ] Mobile app optimization
- [ ] Database backend for large libraries
- [ ] Visualization/spectrum analyzer

## Troubleshooting

### Audio not playing
- Check that audio format is supported (MP3, WAV, FLAC, OGG, M4A)
- Verify file path is correct
- Check system audio is not muted

### WebDAV connection fails
- Verify server URL and credentials
- Check network connectivity
- Ensure WebDAV is enabled on the server

### Playlists not saving
- Check that `playlists/` directory exists and is writable
- Verify disk space is available
- Check file permissions

## License

MIT - See LICENSE file

## Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request
