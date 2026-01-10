# Quick Start Guide - Dioxus Music Player

## Installation & Running

### 1. Install Dioxus CLI
```bash
curl -sSL http://dioxus.dev/install.sh | sh
```

### 2. Build and Run
```bash
cd dioxusmusic
dx serve
```

Open `http://localhost:8080` in your browser.

## First Steps

### Add Music Files

1. Place your music files in a directory (e.g., `~/Music`)
2. Supported formats: MP3, WAV, FLAC, OGG, M4A

### Create a Playlist

1. Click **+ New** button in the playlists sidebar
2. Enter a playlist name (e.g., "My Favorite Songs")
3. Click **Create**

### Add Tracks to Playlist

Currently, the app supports:
- Manual track selection from the UI
- Music file scanning via the `scan_music_directory()` function

To add tracks programmatically:

```rust
use std::path::Path;
use metadata::TrackMetadata;

// Add to src/main.rs or create a data loading function
let track = TrackMetadata::from_file(Path::new("path/to/song.mp3"))?;
// Then add to playlist...
```

### Play Music

1. Select a track from the playlist
2. Click **▶ Play** or it will auto-play when selected
3. Use controls:
   - **⏸ Pause** - Pause playback
   - **⏹ Stop** - Stop playback
   - **Progress bar** - Drag to seek
   - **Volume slider** - Adjust volume (0-100%)

## Features at a Glance

### 🎵 Player Controls
- Play/Pause/Stop
- Seek to any position
- Volume control
- Progress indicator with time display

### 📋 Playlists
- Create multiple playlists
- View all tracks in current playlist
- Click track to play
- Automatic saving (JSON format)

### 🏷️ Track Info
- Title, Artist, Album display
- Duration tracking
- Album cover display (if available in tags)

### ☁️ Cloud Music (WebDAV)
Connect to Nextcloud, Aliyun, or other WebDAV services:

```rust
// In main.rs or a utility function
let client = WebDAVClient::new("https://your-nextcloud.com/...".to_string())
    .with_auth("username".to_string(), "password".to_string());

let files = client.list_files("/music").await?;
```

## Project Files to Explore

| File | Purpose |
|------|---------|
| `src/main.rs` | UI components and app logic |
| `src/player.rs` | Music playback engine |
| `src/playlist.rs` | Playlist management |
| `src/metadata.rs` | Track info extraction |
| `src/webdav.rs` | Cloud storage integration |
| `Cargo.toml` | Dependencies |

## Common Tasks

### Load Music Directory
```bash
# Edit src/main.rs to add:
fn load_music() {
    let tracks = scan_music_directory("~/Music")?;
}
```

### Configure WebDAV
Edit connection in `src/webdav.rs`:
```rust
let webdav = WebDAVClient::new("https://cloud.example.com/webdav/".to_string())
    .with_auth("user".to_string(), "password".to_string());
```

### Save/Load Playlists
```rust
// Save all playlists
save_all_playlists(&playlists, "playlists/")?;

// Load playlists on startup
let playlists = load_all_playlists("playlists/")?;
```

## Keyboard Shortcuts (Future)

These are planned:
- `Space` - Play/Pause
- `Esc` - Stop
- `Right Arrow` - Skip 5 seconds
- `Left Arrow` - Rewind 5 seconds
- `Up/Down` - Volume control

## Troubleshooting

**Audio not playing?**
- Check file format is supported
- Verify file path exists
- Check system volume

**WebDAV not connecting?**
- Verify URL and credentials
- Check firewall/network
- Ensure WebDAV is enabled on server

**Playlists not saving?**
- Check `playlists/` directory writable
- Verify disk space

## Next Steps

1. Add music files to the app
2. Create playlists
3. Test playback controls
4. Try WebDAV cloud music integration
5. Customize UI theme in `assets/tailwind.css`

## Resources

- [Dioxus Documentation](https://dioxuslabs.com/learn/0.7)
- [Rodio Audio](https://github.com/RustAudio/rodio)
- [WebDAV Spec](https://tools.ietf.org/html/rfc4918)

## Support

For issues or questions:
1. Check README.md for detailed documentation
2. Review error messages in browser console
3. Check terminal for Rust compilation errors

Enjoy your music! 🎵
