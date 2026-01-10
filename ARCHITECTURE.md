# Architecture & Technical Documentation

## System Architecture


```
┌─────────────────────────────────────────────────────────┐
│                    UI Layer (Dioxus)                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ PlaylistUI   │  │  PlayerCtrl  │  │ TrackInfo    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────┐
│                  State Management                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Signal       │  │ Signal       │  │ Signal       │  │
│  │ (CurrentTrack)  (Volume)         (PlaybackState) │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────┐
│                  Business Logic Layer                    │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐       │
│  │ Playlist   │  │ Metadata   │  │ WebDAV     │       │
│  │ Manager    │  │ Extractor  │  │ Client     │       │
│  └────────────┘  └────────────┘  └────────────┘       │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────┐
│                  Audio Engine Layer                      │
│  ┌────────────────────────────────────────────────────┐ │
│  │           Rodio (Audio Playback)                    │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐         │ │
│  │  │ Sink     │  │ Sources  │  │ Volume   │         │ │
│  │  └──────────┘  └──────────┘  └──────────┘         │ │
│  └────────────────────────────────────────────────────┘ │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────┴────────────────────────────────┐
│              File System & Network Layer                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Local Files  │  │ JSON Storage │  │ WebDAV HTTP  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Module Description

### `main.rs` - Application Entry Point
**Responsibilities:**
- Dioxus app initialization
- UI component rendering (RSX macros)
- State management with Signals
- Event handlers for user interactions

**Key Components:**
```rust
#[component]
fn App() -> Element {}           // Root component

#[component]
fn NowPlayingCard() -> Element {} // Current track display

#[component]
fn PlayerControls() -> Element {} // Play/pause/seek UI

#[component]
fn PlaylistSidebar() -> Element{} // Playlist selector

#[component]
fn PlaylistTracks() -> Element {}  // Track list view
```

**Signal Types Used:**
- `Signal<PlayerState>` - Current playback state
- `Signal<Option<Track>>` - Currently playing track
- `Signal<Duration>` - Current playback position
- `Signal<f32>` - Volume level (0.0 to 1.0)
- `Signal<Vec<Playlist>>` - All playlists

### `player.rs` - Audio Engine
**Responsibilities:**
- Rodio sink management
- Audio source decoding
- Playback state control
- Volume management

**Key Struct:**
```rust
pub struct MusicPlayer {
    sink: Arc<Mutex<Option<Sink>>>,  // Audio sink
    _stream: OutputStream,             // Audio stream
    current_duration: Arc<Mutex<Duration>>, // Track duration
}
```

**Methods:**
- `new()` - Initialize player
- `play(path)` - Start playback
- `pause()` - Pause audio
- `resume()` - Resume from pause
- `stop()` - Stop playback
- `set_volume(vol)` - Set volume (0.0-1.0)
- `is_paused()` - Check state
- `is_empty()` - Check if playing

**Thread Safety:**
- Uses `Arc<Mutex<>>` for thread-safe state
- Sink is wrapped in Arc for shared ownership
- All methods are `&self` (interior mutability)

### `playlist.rs` - Playlist Management
**Responsibilities:**
- Playlist CRUD operations
- Persistence to JSON files
- Track collection management

**Key Struct:**
```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Playlist {
    pub id: String,              // UUID for unique identification
    pub name: String,            // User-friendly name
    pub tracks: Vec<Track>,      // Collection of tracks
}
```

**Methods:**
- `new(name)` - Create empty playlist
- `add_track(track)` - Add track to playlist
- `remove_track(id)` - Remove by ID
- `clear()` - Remove all tracks
- `save_to_file(path)` - Serialize to JSON
- `load_from_file(path)` - Deserialize from JSON
- `load_multiple_from_dir(dir)` - Batch load

**Persistence:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "My Playlist",
  "tracks": [
    {
      "id": "...",
      "path": "/path/to/song.mp3",
      "title": "Song Name",
      "artist": "Artist Name",
      "album": "Album Name",
      "duration": { "secs": 180, "nanos": 0 },
      "cover": null
    }
  ]
}
```

### `metadata.rs` - Tag Extraction
**Responsibilities:**
- Extract metadata from audio files
- Support multiple tag formats
- Extract album artwork

**Supported Formats:**
- **MP3**: ID3v2 tags (id3 crate)
- **FLAC**: Vorbis comments (metaflac crate)

**Key Function:**
```rust
pub fn extract_metadata(path: &Path) -> Result<Track, Box<dyn Error>>
```

**Extracted Data:**
```rust
pub struct Track {
    pub id: String,              // UUID
    pub path: String,            // File path
    pub title: String,           // Song title
    pub artist: String,          // Artist name
    pub album: String,           // Album name
    pub duration: Duration,      // Track length
    pub cover: Option<Vec<u8>>,  // Album art (JPEG bytes)
}
```

**Priority:**
1. Tries ID3 tags (MP3)
2. Falls back to Vorbis comments (FLAC)
3. Uses filename if no tags found

### `webdav.rs` - Cloud Storage Integration
**Responsibilities:**
- WebDAV protocol communication
- Authentication handling
- File listing and transfer

**Key Struct:**
```rust
pub struct WebDAVClient {
    client: Arc<Client>,          // Shared HTTP client
    base_url: String,             // Server URL
    username: Option<String>,     // Credentials
    password: Option<String>,
}
```

**Methods:**
```rust
pub async fn list_files(&self, path: &str) -> Result<Vec<String>>
pub async fn download_file(&self, remote: &str, local: &str) -> Result<()>
pub async fn upload_file(&self, local: &str, remote: &str) -> Result<()>
```

**Authentication:**
- HTTP Basic Auth support
- Builder pattern for configuration

**PROPFIND Parsing:**
- Simple XML parsing for file listing
- Filters out directories
- Returns file paths

## State Flow

### Playback State Machine
```
        ┌─────────────┐
        │   STOPPED   │◄──────────┐
        └──────┬──────┘           │
               │                  │
          Play │                  │ Stop
               │                  │
               ▼                  │
        ┌─────────────┐           │
        │   PLAYING   ├──────────►│
        └──────┬──────┘  Stop
               │
          Pause│
               │
               ▼
        ┌─────────────┐
        │   PAUSED    │
        └──────┬──────┘
               │
          Resume/
          Play │
               │
               └──► (back to PLAYING)
```

### Data Flow
```
User selects track
        │
        ▼
Update Signal<Option<Track>>
        │
        ├─ NowPlayingCard component re-renders
        ├─ PlayerControls update duration
        │
        ▼
User clicks Play
        │
        ├─ Update Signal<PlayerState> to Playing
        │
        ▼
MusicPlayer.play(track.path) called
        │
        ├─ Rodio decoder reads audio file
        ├─ Sink appends source
        ├─ Audio streams to system output
        │
        ▼
Progress updates (future: via coroutine)
        │
        └─ Signal<Duration> updates UI progress bar
```

## Thread & Concurrency Model

### Dioxus Runtime
- Single-threaded for UI rendering
- Signals automatically trigger re-renders
- EventHandlers run on main thread

### Audio Playback
- Rodio runs audio in background thread
- Uses Arc<Mutex<>> for thread-safe sink access
- Non-blocking play() calls

### File I/O
- Blocking on main thread (future: async with tokio)
- Playlist save/load are synchronous
- WebDAV uses tokio async runtime

### Async Operations (Future)
```rust
use_coroutine(move |_rx| async move {
    // Background task for progress updates
    loop {
        update_progress();
        sleep(100ms).await;
    }
});
```

## Error Handling

### Error Types Used
```rust
Result<T, Box<dyn std::error::Error>>
```

**Error Sources:**
- File I/O errors (file not found, permission denied)
- Audio decoding errors (unsupported format)
- JSON serialization errors
- Network errors (WebDAV)
- Metadata extraction errors (missing tags)

### Error Propagation
```rust
pub fn some_function() -> Result<T, Box<dyn std::error::Error>> {
    let file = File::open(path)?;      // ? operator propagates
    let metadata = extract_metadata()?;
    Ok(metadata)
}
```

## Performance Considerations

### Audio Streaming
- Rodio streams audio (not loaded to memory)
- Efficient for large files
- Hardware-accelerated on available platforms

### Metadata Caching
- Extracted metadata stored in Track struct
- Reused across playlists
- UUIDs prevent duplicate processing

### UI Rendering
- Dioxus only re-renders changed signals
- PlaylistTracks uses iterator mapping for lazy eval
- Tailwind CSS provides efficient styling

### Optimizations
```rust
// Lazy iteration - doesn't allocate
for (idx, track) in playlist.tracks.iter().enumerate() {
    rsx! { /* render */ }
}

// Memoization (future)
let playlist_size = use_memo(move || playlists().len());
```

## Extensibility Points

### Adding New Features
1. **New audio format**: Extend metadata.rs
2. **New cloud service**: Create new module, implement same API
3. **New UI component**: Add to main.rs as new #[component]
4. **New playback mode**: Add PlayerMode enum, update player.rs

### Plugin Architecture (Future)
```rust
pub trait MusicSource {
    async fn list_files(&self, path: &str) -> Result<Vec<Track>>;
    async fn get_track(&self, track: &Track) -> Result<Vec<u8>>;
}

// Implement for LocalFS, WebDAV, Spotify, etc.
```

## Build Targets

### Web Build
- Compiles to WebAssembly
- Runs in browser
- Uses web audio API via Rodio

### Desktop Build  
- Native executable
- Uses OS audio APIs (CoreAudio on macOS, ALSA on Linux, WASAPI on Windows)
- Direct filesystem access

### Mobile Build
- iOS/Android native
- Platform-specific audio APIs
- Touch UI optimizations

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_playlist_operations() { }
    
    #[test]
    fn test_metadata_extraction() { }
}
```

### Integration Tests
- Test full playback flow
- Test playlist persistence
- Test WebDAV integration

### Manual Testing
- Cross-format compatibility
- UI responsiveness
- Cloud service integration

## Future Improvements

1. **Performance**
   - Async metadata extraction
   - Background playlist loading
   - Database backend for large libraries

2. **Features**
   - Shuffle/repeat modes
   - Queue management
   - Search and filters
   - Visualizations

3. **UX**
   - Drag-and-drop UI
   - Keyboard shortcuts
   - Dark/light themes
   - Mobile optimization

4. **Architecture**
   - Plugin system
   - Event bus for decoupled components
   - Dependency injection
