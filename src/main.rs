mod player;
mod playlist;
mod metadata;
mod webdav;

use dioxus::prelude::*;
use player::{MusicPlayer, PlayerState};
use playlist::Playlist;
use metadata::TrackMetadata;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use uuid::Uuid;
use std::sync::{Arc, Mutex};

// Global state for auto-play detection - shared across threads
#[derive(Clone, Default)]
pub struct GlobalPlayerState {
    pub last_track_ended: Arc<Mutex<bool>>,
    pub last_track_id: Arc<Mutex<Option<String>>>,
}

impl GlobalPlayerState {
    pub fn new() -> Self {
        Self {
            last_track_ended: Arc::new(Mutex::new(false)),
            last_track_id: Arc::new(Mutex::new(None)),
        }
    }
    
    pub fn set_last_track(&self, id: String) {
        *self.last_track_id.lock().unwrap() = Some(id);
    }
    
    pub fn get_last_track(&self) -> Option<String> {
        self.last_track_id.lock().unwrap().clone()
    }
}

// Global state singleton
static GLOBAL_STATE: std::sync::OnceLock<GlobalPlayerState> = std::sync::OnceLock::new();

fn get_global_state() -> &'static GlobalPlayerState {
    GLOBAL_STATE.get_or_init(GlobalPlayerState::new)
}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

// Supported audio formats
const AUDIO_FORMATS: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
    pub cover: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrackStub {
    pub id: String,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
}

impl From<Track> for TrackStub {
    fn from(track: Track) -> Self {
        TrackStub {
            id: track.id,
            path: track.path,
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration: track.duration,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WebDAVConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub enabled: bool,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut player_state = use_signal(|| PlayerState::Stopped);
    let mut current_track = use_signal(|| None::<TrackStub>);
    let mut current_time = use_signal(|| Duration::from_secs(0));
    let mut volume = use_signal(|| 0.7);
    let mut playlists = use_signal(|| vec![Playlist::new("My Playlist".to_string())]);
    let mut current_playlist = use_signal(|| 0);
    let mut show_playlist_manager = use_signal(|| false);
    let mut show_directory_browser = use_signal(|| false);
    let mut show_webdav_config = use_signal(|| false);
    let mut show_webdav_config_list = use_signal(|| false);
    let mut show_webdav_browser = use_signal(|| false);
    let mut webdav_configs = use_signal(|| load_webdav_configs().unwrap_or_default());
    let mut current_webdav_config = use_signal(|| None::<usize>);
    let mut editing_webdav_config = use_signal(|| None::<usize>);
    let mut current_directory = use_signal(|| String::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_string())));
    let mut error_msg = use_signal(|| None::<String>);
    
    // Provide current_time as context for child components
    provide_context(current_time);

    // WebDAV Browser State
    let mut webdav_current_path = use_signal(|| "/".to_string());
    let mut webdav_items = use_signal(|| Vec::<webdav::WebDAVItem>::new());
    let mut webdav_is_loading = use_signal(|| false);
    let mut webdav_error = use_signal(|| Option::<String>::None);
    
    // Auto-play trigger - atomic counter for thread-safe triggering
    let track_check_trigger: &'static Arc<std::sync::atomic::AtomicUsize> = {
        static TRIGGER: std::sync::OnceLock<Arc<std::sync::atomic::AtomicUsize>> = std::sync::OnceLock::new();
        TRIGGER.get_or_init(|| Arc::new(std::sync::atomic::AtomicUsize::new(0)))
    };
    
    // Create a static-like player reference stored in component state
    // This will be created once and persist for the lifetime of the app
    let player_ref = use_signal(|| MusicPlayer::new().ok());
    
    // Auto-play: periodically check if track ended and update current time
    let global_state = get_global_state().clone();
    let player_ref_clone = player_ref.clone();
    
    let _time_update_future = use_future(move || {
        let global_state = global_state.clone();
        let player_ref_clone = player_ref_clone.clone();
        
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                
                let player_guard = player_ref_clone.read();
                if let Some(player) = player_guard.as_ref() {
                    // Update current time
                    let elapsed = player.get_elapsed();
                    *current_time.write() = elapsed;
                    
                    // Check for track end
                    let is_ended = *player.track_ended.lock().unwrap();
                    let was_stopped_by_user = *player.stopped_by_user.lock().unwrap();
                    if is_ended {
                        eprintln!("[UI] 检测到曲目结束, stopped_by_user={}", was_stopped_by_user);
                        
                        // Reset the flags
                        *player.track_ended.lock().unwrap() = false;
                        *player.stopped_by_user.lock().unwrap() = false;
                        
                        if !was_stopped_by_user {
                            eprintln!("[UI] 检测到曲目自然结束");
                            
                            let last_track_id = player.get_last_track_id();
                            if let Some(id) = last_track_id {
                                // Clone for the global state and keep original for closure
                                global_state.set_last_track(id.clone());
                                let track_id_for_search = id.clone();
                                
                                let all_playlists = playlists();
                                let current_playlist_idx = current_playlist();
                                
                                if all_playlists.len() > current_playlist_idx {
                                    let playlist = &all_playlists[current_playlist_idx];
                                    if let Some(pos) = playlist.tracks.iter().position(|t| t.id == track_id_for_search) {
                                        if pos < playlist.tracks.len() - 1 {
                                            let next_track = playlist.tracks[pos + 1].clone();
                                            eprintln!("[UI] 自动播放下一首: {}", next_track.title);
                                            
                                            let path = std::path::Path::new(&next_track.path);
                                            let _ = player.play(path, Some(next_track.id.clone()));
                                            player.set_stopped_by_user(false);
                                            let vol = *volume.read();
                                            let _ = player.set_volume(vol);
                                            
                                            *current_track.write() = Some(TrackStub::from(next_track.clone()));
                                            *player_state.write() = PlayerState::Playing;
                                        } else {
                                            eprintln!("[UI] 播放列表已结束");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    
    // We'll access it directly in the closures since Signal is Copy

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div { class: "min-h-screen bg-gradient-to-b from-gray-900 to-black text-white",

            header { class: "bg-gray-800 shadow-lg p-6",
                div { class: "max-w-7xl mx-auto",
                    h1 { class: "text-4xl font-bold mb-2", "🎵 Dioxus Music Player" }
                    p { class: "text-gray-400",
                        "Control your music with play, pause, seek, and playlist management"
                    }
                    div { class: "mt-4 flex gap-2",
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded text-sm",
                            onclick: move |_| *show_directory_browser.write() = true,
                            "📁 Add Music"
                        }
                        button {
                            class: "px-4 py-2 bg-purple-600 hover:bg-purple-700 rounded text-sm",
                            onclick: move |_| *show_webdav_config_list.write() = true,
                            "☁️ WebDAV Config"
                        }
                        if current_webdav_config().is_some()
                            && webdav_configs().len() > current_webdav_config().unwrap_or(0)
                        {
                            button {
                                class: "px-4 py-2 bg-teal-600 hover:bg-teal-700 rounded text-sm",
                                onclick: move |_| {
                                    *show_webdav_browser.write() = true;
                                    // Initial load if empty and config exists
                                    if webdav_items.read().is_empty() {
                                        if let Some(idx) = current_webdav_config() {
                                            if idx < webdav_configs.read().len() {
                                                let cfg = webdav_configs.read()[idx].clone();
                                                let path = webdav_current_path();
                                                *webdav_is_loading.write() = true;
                                                spawn(async move {
                                                    match load_webdav_folder(&cfg, &path).await {
                                                        Ok(items) => {
                                                            *webdav_items.write() = items;
                                                            *webdav_error.write() = None;
                                                        }
                                                        Err(e) => {
                                                            *webdav_error.write() = Some(format!("Error: {}", e));
                                                        }
                                                    }
                                                    *webdav_is_loading.write() = false;
                                                });
                                            }
                                        }
                                    }
                                },
                                "🌐 Browse Cloud"
                            }
                        }
                    }
                }
            }

            main { class: "max-w-7xl mx-auto p-6",

                div { class: "grid grid-cols-3 gap-6",

                    aside { class: "col-span-1 h-[calc(100vh-12rem)]",
                        if show_webdav_browser() {
                            if let Some(config_idx) = current_webdav_config() {
                                if config_idx < webdav_configs().len() {
                                    WebDAVSidebar {
                                        config: webdav_configs()[config_idx].clone(),
                                        current_path: webdav_current_path(),
                                        items: webdav_items(),
                                        is_loading: webdav_is_loading(),
                                        error_msg: webdav_error(),
                                        on_close: move |_| *show_webdav_browser.write() = false,
                                        on_navigate: move |path: String| {
                                            *webdav_current_path.write() = path.clone();
                                            *webdav_is_loading.write() = true;
                                            let cfg = webdav_configs()[config_idx].clone();
                                            spawn(async move {
                                                match load_webdav_folder(&cfg, &path).await {
                                                    Ok(items) => {
                                                        *webdav_items.write() = items;
                                                        *webdav_error.write() = None;
                                                    }
                                                    Err(e) => {
                                                        *webdav_error.write() = Some(format!("Error: {}", e));
                                                    }
                                                }
                                                *webdav_is_loading.write() = false;
                                            });
                                        },
                                        on_play_track: move |item: webdav::WebDAVItem| {
                                            let cfg = webdav_configs()[config_idx].clone();
                                            let current_items = webdav_items();
                                            let audio_files: Vec<String> = current_items

                                                .iter()
                                                .filter(|i| !i.is_dir && is_audio_file(&i.name))
                                                .map(|i| i.path.clone())
                                                .collect();
                                            spawn(async move {
                                                // Create placeholder tracks without downloading
                                                if let Ok(tracks) = create_webdav_placeholder_tracks(&cfg, &audio_files)
                                                    .await
                                                {
                                                    if !tracks.is_empty() {
                                                        if playlists().len() > current_playlist() {
                                                            let mut plist = playlists()[current_playlist()].clone();
                                                            let mut target_track_id = None;
                                                            let target_path = item.path.clone();
                                                            for track in tracks {
                                                                if track.path == target_path {
                                                                    target_track_id = Some(track.id.clone());
                                                                }
                                                                plist.add_track(track.into());
                                                            }
                                                            let mut lists = playlists.write();
                                                            lists[current_playlist()] = plist;
                                                            if let Some(id) = target_track_id {
                                                                if let Some(track) = lists[current_playlist()].get_track(&id)
                                                                {
                                                                    let stub = TrackStub::from(track.clone());
                                                                    if let Some(ref player) = *player_ref.read() {
                                                                        let _ = player
                                                                            .play(
                                                                                std::path::Path::new(&track.path),
                                                                                Some(track.id.clone()),
                                                                            );
                                                                        let _ = player.set_volume(volume());
                                                                    }
                                                                    *current_track.write() = Some(stub);
                                                                    *player_state.write() = PlayerState::Playing;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        },
                                    }
                                } else {
                                    div { "Invalid Config" }
                                }
                            } else {
                                div { "No Config Selected" }
                            }
                        } else {
                            PlaylistSidebar {
                                playlists: playlists(),
                                current_playlist: current_playlist(),
                                webdav_configs: webdav_configs(),
                                expanded_webdav_index: current_webdav_config(),
                                webdav_items: webdav_items(),
                                webdav_current_path: webdav_current_path(),
                                webdav_loading: webdav_is_loading(),
                                on_select: move |idx| {
                                    *current_playlist.write() = idx;
                                },
                                on_add_playlist: move |_| {
                                    *show_playlist_manager.write() = true;
                                },
                                on_toggle_webdav: move |idx| {
                                    // If clicking the same one, collapse it
                                    if current_webdav_config() == Some(idx) {
                                        *current_webdav_config.write() = None;
                                    } else {
                                        // Expand new one
                                        *current_webdav_config.write() = Some(idx);
                                        *webdav_current_path.write() = "/".to_string();

                                        // Trigger initial load
                                        if idx < webdav_configs().len() {
                                            let cfg = webdav_configs()[idx].clone();
                                            *webdav_is_loading.write() = true;
                                            spawn(async move {
                                                match load_webdav_folder(&cfg, "/").await {
                                                    Ok(items) => {
                                                        *webdav_items.write() = items;
                                                        *webdav_error.write() = None;
                                                    }
                                                    Err(e) => {
                                                        *webdav_error.write() = Some(format!("Error: {}", e));
                                                    }
                                                }
                                                *webdav_is_loading.write() = false;
                                            });
                                        }
                                    }
                                },
                                on_webdav_navigate: move |path: String| {
                                    *webdav_current_path.write() = path.clone();
                                    *webdav_is_loading.write() = true;

                                    if let Some(config_idx) = current_webdav_config() {
                                        if config_idx < webdav_configs().len() {
                                            let cfg = webdav_configs()[config_idx].clone();
                                            spawn(async move {
                                                match load_webdav_folder(&cfg, &path).await {
                                                    Ok(items) => {
                                                        *webdav_items.write() = items;
                                                        *webdav_error.write() = None;
                                                    }
                                                    Err(e) => {
                                                        *webdav_error.write() = Some(format!("Error: {}", e));
                                                    }
                                                }
                                                *webdav_is_loading.write() = false;
                                            });
                                        }
                                    }
                                },
                                on_webdav_play: move |item: webdav::WebDAVItem| {
                                    if let Some(config_idx) = current_webdav_config() {
                                        if config_idx < webdav_configs().len() {
                                            let cfg = webdav_configs()[config_idx].clone();
                                            let current_items = webdav_items();

                                            // Get all audio files in current directory
                                            let audio_files: Vec<String> = current_items
                                                .iter()
                                                .filter(|i| !i.is_dir && is_audio_file(&i.name))
                                                .map(|i| i.path.clone())
                                                .collect();
                                            spawn(async move {
                                                // Create placeholder tracks without downloading
                                                if let Ok(tracks) = create_webdav_placeholder_tracks(
                                                        &cfg,
                                                        &audio_files,
                                                    )
                                                    .await
                                                {
                                                    if !tracks.is_empty() {
                                                        if playlists().len() > current_playlist() {
                                                            let mut plist = playlists()[current_playlist()].clone();
                                                            let mut target_track_id = None;
                                                            let target_path = item.path.clone();
                                                            for track in tracks {
                                                                if track.path == target_path {
                                                                    target_track_id = Some(track.id.clone());
                                                                }
                                                                plist.add_track(track.into());
                                                            }
                                                            let mut lists = playlists.write();
                                                            lists[current_playlist()] = plist;
                                                            if let Some(id) = target_track_id {
                                                                if let Some(track) = lists[current_playlist()]
                                                                    .get_track(&id)
                                                                {
                                                                    let stub = TrackStub::from(track.clone());
                                                                    if let Some(ref player) = *player_ref.read() {
                                                                        let _ = player
                                                                            .play(
                                                                                std::path::Path::new(&track.path),
                                                                                Some(track.id.clone()),
                                                                            );
                                                                        let _ = player.set_volume(volume());
                                                                    }
                                                                    *current_track.write() = Some(stub);
                                                                    *player_state.write() = PlayerState::Playing;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    }
                                },
                            }
                        }
                    }

                    section { class: "col-span-2",

                        NowPlayingCard {
                            current_track: current_track(),
                            player_ref: player_ref.clone(),
                        }

                        // Error message display
                        if let Some(err) = error_msg() {
                            div { class: "mb-4 p-4 bg-red-100 border border-red-400 text-red-700 rounded",
                                "❌ {err}"
                                button {
                                    class: "ml-2 text-red-500 hover:text-red-700",
                                    onclick: move |_| *error_msg.write() = None,
                                    "✕"
                                }
                            }
                        }

                        PlayerControls {
                            state: player_state(),
                            duration: current_track().as_ref().map(|t| t.duration),
                            volume: volume(),
                            current_time,
                            on_play: move |_| {
                                // Read the current player value
                                if let Some(ref player) = *player_ref.read() {
                                    // Reset stopped by user flag
                                    player.set_stopped_by_user(false);

                                    // If paused, resume from where we left off
                                    // Otherwise, play from the beginning
                                    if player_state() == PlayerState::Paused && player.is_paused() {
                                        let _ = player.resume();
                                    } else if let Some(track_stub) = current_track() {
                                        match player
                                            .play(
                                                std::path::Path::new(&track_stub.path),
                                                Some(track_stub.id.clone()),
                                            )
                                        {
                                            Ok(_) => {
                                                let _ = player.set_volume(volume());
                                            }
                                            Err(e) => {
                                                *error_msg.write() = Some(format!("播放失败: {}", e));
                                            }
                                        }
                                    }
                                }
                                *player_state.write() = PlayerState::Playing;
                            },
                            on_pause: move |_| {
                                if let Some(ref player) = *player_ref.read() {
                                    let _ = player.pause();
                                }
                                *player_state.write() = PlayerState::Paused;
                            },
                            on_stop: move |_| {
                                if let Some(ref player) = *player_ref.read() {
                                    player.set_stopped_by_user(true);
                                    let _ = player.stop();
                                }
                                *player_state.write() = PlayerState::Stopped;
                            },
                            on_seek: move |time| {
                                if let Some(ref player) = *player_ref.read() {
                                    let _ = player.seek(time);
                                }
                                *current_time.write() = time;
                            },
                            on_volume_change: move |vol| {
                                if let Some(ref player) = *player_ref.read() {
                                    let _ = player.set_volume(vol);
                                }
                                *volume.write() = vol;
                            },
                            on_previous: move |_| {
                                if playlists().len() > current_playlist() {
                                    let playlist = &playlists()[current_playlist()];
                                    if let Some(current) = current_track() {
                                        // Find current track index
                                        if let Some(pos) = playlist
                                            .tracks
                                            .iter()
                                            .position(|t| t.id == current.id)
                                        {
                                            if pos > 0 {
                                                let prev_track = playlist.tracks[pos - 1].clone();
                                                if let Some(ref player) = *player_ref.read() {
                                                    player.set_stopped_by_user(false);
                                                    match player
                                                        .play(
                                                            std::path::Path::new(&prev_track.path),
                                                            Some(prev_track.id.clone()),
                                                        )
                                                    {
                                                        Ok(_) => {
                                                            let _ = player.set_volume(volume());
                                                        }
                                                        Err(e) => {
                                                            *error_msg.write() = Some(
                                                                format!("播放上一首失败: {}", e),
                                                            );
                                                        }
                                                    }
                                                }
                                                *current_track.write() = Some(prev_track);
                                                *player_state.write() = PlayerState::Playing;
                                            }
                                        }
                                    }
                                }
                            },
                            on_next: move |_| {
                                if playlists().len() > current_playlist() {
                                    let playlist = &playlists()[current_playlist()];
                                    if let Some(current) = current_track() {
                                        // Find current track index
                                        if let Some(pos) = playlist
                                            .tracks
                                            .iter()
                                            .position(|t| t.id == current.id)
                                        {
                                            if pos < playlist.tracks.len() - 1 {
                                                let next_track = playlist.tracks[pos + 1].clone();
                                                if let Some(ref player) = *player_ref.read() {
                                                    player.set_stopped_by_user(false);
                                                    match player
                                                        .play(
                                                            std::path::Path::new(&next_track.path),
                                                            Some(next_track.id.clone()),
                                                        )
                                                    {
                                                        Ok(_) => {
                                                            let _ = player.set_volume(volume());
                                                        }
                                                        Err(e) => {
                                                            *error_msg.write() = Some(
                                                                format!("播放下一首失败: {}", e),
                                                            );
                                                        }
                                                    }
                                                }
                                                *current_track.write() = Some(next_track);
                                                *player_state.write() = PlayerState::Playing;
                                            }
                                        }
                                    }
                                }
                            },
                        }

                        if playlists().len() > current_playlist() {
                            PlaylistTracks {
                                playlist: playlists()[current_playlist()].clone(),
                                current_track: current_track(),
                                on_track_select: move |track_stub: TrackStub| {
                                    if let Some(ref player) = *player_ref.read() {
                                        player.set_stopped_by_user(false);
                                        match player
                                            .play(
                                                std::path::Path::new(&track_stub.path),
                                                Some(track_stub.id.clone()),
                                            )
                                        {
                                            Ok(_) => {
                                                let _ = player.set_volume(volume());
                                            }
                                            Err(e) => {
                                                *error_msg.write() = Some(format!("播放失败: {}", e));
                                            }
                                        }
                                    }
                                    *current_track.write() = Some(track_stub);
                                    *player_state.write() = PlayerState::Playing;
                                },
                            }
                        }
                    }
                }
            }

            if show_playlist_manager() {
                PlaylistManagerModal {
                    on_close: move |_| {
                        *show_playlist_manager.write() = false;
                    },
                    on_add_playlist: move |name| {
                        let new_playlist = Playlist::new(name);
                        playlists.write().push(new_playlist);
                        *show_playlist_manager.write() = false;
                    },
                    on_load_files: move |_| {},
                }
            }

            if show_directory_browser() {
                DirectoryBrowserModal {
                    current_directory: current_directory(),
                    on_close: move |_| {
                        *show_directory_browser.write() = false;
                    },
                    on_load_directory: move |dir: String| {
                        *current_directory.write() = dir.clone();
                        if let Ok(tracks) = scan_music_directory(&dir) {
                            if playlists().len() > current_playlist() {
                                let mut plist = playlists()[current_playlist()].clone();
                                for track in tracks {
                                    plist.add_track(track);
                                }
                                let mut lists = playlists.write();
                                lists[current_playlist()] = plist;
                            }
                        }
                        *show_directory_browser.write() = false;
                    },
                }
            }

            if show_webdav_config_list() {
                WebDAVConfigListModal {
                    configs: webdav_configs(),
                    current_config: current_webdav_config(),
                    on_close: move |_| {
                        *show_webdav_config_list.write() = false;
                    },
                    on_add_config: move |_| {
                        *editing_webdav_config.write() = None;
                        *show_webdav_config.write() = true;
                    },
                    on_edit_config: move |idx| {
                        *editing_webdav_config.write() = Some(idx);
                        *show_webdav_config.write() = true;
                    },
                    on_delete_config: move |idx| {
                        let mut configs = webdav_configs.write();
                        if idx < configs.len() {
                            configs.remove(idx);
                        }
                        if let Some(current) = current_webdav_config() {
                            if current >= configs.len() && !configs.is_empty() {
                                *current_webdav_config.write() = Some(configs.len() - 1);
                            }
                        }
                    },
                    on_select_config: move |idx| {
                        *current_webdav_config.write() = Some(idx);
                    },
                }
            }

            if show_webdav_config() {
                WebDAVConfigModal {
                    config: {
                        let editing_idx = editing_webdav_config();
                        if let Some(idx) = editing_idx {
                            if idx < webdav_configs().len() {
                                webdav_configs()[idx].clone()
                            } else {
                                WebDAVConfig {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    name: String::new(),
                                    url: String::new(),
                                    username: String::new(),
                                    password: String::new(),
                                    enabled: false,
                                }
                            }
                        } else {
                            WebDAVConfig {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: String::new(),
                                url: String::new(),
                                username: String::new(),
                                password: String::new(),
                                enabled: false,
                            }
                        }
                    },
                    on_close: move |_| {
                        *show_webdav_config.write() = false;
                        *editing_webdav_config.write() = None;
                    },
                    on_save_config: move |new_config| {
                        let editing_idx = editing_webdav_config();
                        if let Some(idx) = editing_idx {
                            if idx < webdav_configs().len() {
                                let mut configs = webdav_configs.write();
                                configs[idx] = new_config;
                            }
                        } else {
                            webdav_configs.write().push(new_config);
                        }
                        *show_webdav_config.write() = false;
                        *editing_webdav_config.write() = None;
                        *show_webdav_config_list.write() = true;
                    },
                }
            }

            if show_webdav_browser() {
                if let Some(config_idx) = current_webdav_config() {
                    if config_idx < webdav_configs().len() {
                        {
                            rsx! {
                                WebDAVBrowserModal {
                                    config: webdav_configs()[config_idx].clone(),
                                    on_close: move |_| {
                                        *show_webdav_browser.write() = false;
                                    },
                                    on_import_folder: move |tracks: Vec<Track>| {
                                        if playlists().len() > current_playlist() {
                                            let mut plist = playlists()[current_playlist()].clone();
                                            for track in tracks {
                                                plist.add_track(track.into());
                                            }
                                            let mut lists = playlists.write();
                                            lists[current_playlist()] = plist;
                                        }
                                        *show_webdav_browser.write() = false;
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NowPlayingCard(
    current_track: Option<TrackStub>,
    player_ref: Signal<Option<player::MusicPlayer>>,
) -> Element {
    let full_track: Option<Track> = current_track.as_ref().map(|stub| {
        Track {
            id: stub.id.clone(),
            path: stub.path.clone(),
            title: stub.title.clone(),
            artist: stub.artist.clone(),
            album: stub.album.clone(),
            duration: stub.duration,
            cover: None,
        }
    });

    let mut player_metadata: Signal<Option<player::TrackMetadata>> = use_signal(|| None);

    let _metadata_future = use_future(move || {
        let player_ref = player_ref.clone();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                if let Some(ref player) = *player_ref.read() {
                    if let Some(metadata) = player.get_current_metadata() {
                        *player_metadata.write() = Some(metadata);
                    }
                }
            }
        }
    });

    let cover_img = player_metadata().as_ref()
        .and_then(|m| m.cover.as_ref())
        .or_else(|| full_track.as_ref().and_then(|t| t.cover.as_ref()))
        .map(|cover_data| {
            let base64_cover = base64_encode(cover_data);
            format!("data:image/jpeg;base64,{}", base64_cover)
        });

    let display_title = player_metadata().as_ref()
        .and_then(|m| m.title.clone())
        .or_else(|| full_track.as_ref().map(|t| t.title.clone()))
        .unwrap_or_else(|| "Unknown".to_string());

    let display_artist = player_metadata().as_ref()
        .and_then(|m| m.artist.clone())
        .or_else(|| full_track.as_ref().map(|t| t.artist.clone()))
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let display_album = player_metadata().as_ref()
        .and_then(|m| m.album.clone())
        .or_else(|| full_track.as_ref().map(|t| t.album.clone()))
        .unwrap_or_else(|| "Unknown Album".to_string());

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-8 mb-6 text-center",

            if let Some(img_src) = cover_img {
                div { class: "w-48 h-48 mx-auto mb-4 rounded-lg shadow-lg overflow-hidden",
                    img {
                        src: img_src,
                        alt: "Album cover",
                        class: "w-full h-full object-cover",
                    }
                }
            } else {
                div { class: "w-48 h-48 mx-auto mb-4 rounded-lg shadow-lg bg-gray-700 flex items-center justify-center text-4xl",
                    "🎵"
                }
            }
            h2 { class: "text-2xl font-bold mb-2", "{display_title}" }
            p { class: "text-gray-400 mb-1", "{display_artist}" }
            p { class: "text-gray-500 text-sm", "{display_album}" }
        }
    }
}

#[component]
fn PlayerControls(
    state: PlayerState,
    duration: Option<Duration>,
    volume: f32,
    current_time: Signal<Duration>,
    on_play: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_stop: EventHandler<()>,
    on_seek: EventHandler<Duration>,
    on_volume_change: EventHandler<f32>,
    on_previous: EventHandler<()>,
    on_next: EventHandler<()>,
) -> Element {
    let ct = current_time();
    let formatted_time = format_duration(ct);
    let formatted_duration = duration.map(format_duration).unwrap_or_else(|| "0:00".to_string());
    let progress_percent = if let Some(d) = duration {
        if d.as_secs() > 0 {
            (ct.as_secs_f64() / d.as_secs_f64() * 100.0) as i32
        } else {
            0
        }
    } else {
        0
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 mb-6",

            div { class: "mb-4",
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: progress_percent,
                    class: "w-full h-2 bg-gray-700 rounded-full appearance-none cursor-pointer",
                    style: "background: linear-gradient(to right, #3b82f6 {progress_percent}%, #374151 {progress_percent}%);",
                    oninput: move |e| {
                        if let Some(d) = duration {
                            let percent = e.value().parse::<f64>().unwrap_or(0.0) / 100.0;
                            let seek_time = Duration::from_secs_f64(d.as_secs_f64() * percent);
                            on_seek.call(seek_time);
                        }
                    },
                }
                div { class: "flex justify-between mt-2 text-xs text-gray-400",
                    span { "{formatted_time}" }
                    span { "{formatted_duration}" }
                }
            }

            div { class: "flex justify-center items-center gap-4 mb-6",

                button {
                    class: "px-6 py-2 bg-blue-500 hover:bg-blue-600 rounded-lg font-semibold",
                    onclick: move |_| on_previous.call(()),
                    "⏮ Previous"
                }

                button {
                    class: "px-6 py-2 bg-red-500 hover:bg-red-600 rounded-lg font-semibold",
                    onclick: move |_| on_stop.call(()),
                    "⏹ Stop"
                }

                if state == PlayerState::Playing {
                    button {
                        class: "px-6 py-2 bg-yellow-500 hover:bg-yellow-600 rounded-lg font-semibold text-black",
                        onclick: move |_| on_pause.call(()),
                        "⏸ Pause"
                    }
                } else {
                    button {
                        class: "px-6 py-2 bg-green-500 hover:bg-green-600 rounded-lg font-semibold text-black",
                        onclick: move |_| on_play.call(()),
                        "▶ Play"
                    }
                }

                button {
                    class: "px-6 py-2 bg-blue-500 hover:bg-blue-600 rounded-lg font-semibold",
                    onclick: move |_| on_next.call(()),
                    "⏭ Next"
                }
            }

            div { class: "flex items-center gap-4",
                span { class: "text-sm", "🔊" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: (volume * 100.0) as i32,
                    class: "flex-1",
                    oninput: move |e| {
                        let val = e.value().parse::<f32>().unwrap_or(1.0) / 100.0;
                        on_volume_change.call(val);
                    },
                }
                span { class: "text-sm w-8", "{(volume * 100.0) as i32}%" }
            }
        }
    }
}

#[component]
fn PlaylistSidebar(
    playlists: Vec<Playlist>,
    current_playlist: usize,
    webdav_configs: Vec<WebDAVConfig>,
    expanded_webdav_index: Option<usize>,
    webdav_items: Vec<webdav::WebDAVItem>,
    webdav_current_path: String,
    webdav_loading: bool,
    on_select: EventHandler<usize>,
    on_add_playlist: EventHandler<()>,
    on_toggle_webdav: EventHandler<usize>,
    on_webdav_navigate: EventHandler<String>,
    on_webdav_play: EventHandler<webdav::WebDAVItem>,
) -> Element {
    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4 h-full flex flex-col",

            div { class: "flex-1 overflow-y-auto mb-4",
                div { class: "flex justify-between items-center mb-4",
                    h3 { class: "text-lg font-bold", "📋 Playlists" }
                    button {
                        class: "px-3 py-1 bg-blue-500 hover:bg-blue-600 rounded text-sm",
                        onclick: move |_| on_add_playlist.call(()),
                        "+ New"
                    }
                }

                div { class: "space-y-2",
                    for (idx , playlist) in playlists.iter().enumerate() {
                        button {
                            class: if idx == current_playlist { "w-full text-left px-3 py-2 rounded bg-blue-600 hover:bg-blue-700 text-sm" } else { "w-full text-left px-3 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm" },
                            onclick: move |_| on_select.call(idx),
                            div { class: "font-semibold", "{playlist.name}" }
                            p { class: "text-xs text-gray-300", "{playlist.tracks.len()} track(s)" }
                        }
                    }
                }
            }

            // WebDAV Servers Section
            if !webdav_configs.is_empty() {
                div { class: "border-t border-gray-700 pt-4 max-h-[50%]",
                    h3 { class: "text-lg font-bold mb-2", "☁️ Cloud Sources" }
                    div { class: "space-y-2 overflow-y-auto",
                        for (idx , config) in webdav_configs.iter().enumerate() {
                            if config.enabled {
                                div { class: "mb-2",
                                    button {
                                        class: "w-full text-left px-3 py-2 rounded bg-gray-700 hover:bg-teal-700 text-sm flex items-center gap-2 mb-1",
                                        onclick: move |_| on_toggle_webdav.call(idx),
                                        span { "☁️" }
                                        div {
                                            div { class: "font-semibold truncate", "{config.name}" }
                                            div { class: "text-xs text-gray-400 truncate",
                                                "{config.url}"
                                            }
                                        }
                                    }

                                    if expanded_webdav_index == Some(idx) {
                                        div { class: "ml-4 border-l-2 border-gray-600 pl-2 space-y-1",
                                            if webdav_loading {
                                                div { class: "text-xs text-gray-400 p-2",
                                                    "🔄 Loading..."
                                                }
                                            } else {
                                                // Breadcrumb / Up Navigation
                                                {
                                                    if webdav_current_path != "/" {
                                                        let nav_path = webdav_current_path.clone();
                                                        Some(rsx! {
                                                            button {
                                                                class: "w-full text-left px-2 py-1 text-xs bg-gray-600 hover:bg-gray-500 rounded mb-1",
                                                                onclick: move |_| {
                                                                    let mut path = nav_path.clone();
                                                                    if path.ends_with('/') {
                                                                        path.pop();
                                                                    }
                                                                    if let Some(pos) = path.rfind('/') {
                                                                        path.truncate(pos + 1);
                                                                    } else {
                                                                        path = "/".to_string();
                                                                    }
                                                                    on_webdav_navigate.call(path);
                                                                },
                                                                "⬆ .."
                                                            }
                                                        })
                                                    } else {
                                                        None
                                                    }
                                                }

                                                if webdav_items.is_empty() {
                                                    div { class: "text-xs text-gray-400 p-2",
                                                        "Empty folder"
                                                    }
                                                } else {
                                                    {

                                                        webdav_items
                                                            .iter()
                                                            .map(|item| {
                                                                let item_clone = item.clone();
                                                                let is_dir = item.is_dir;
                                                                let item_name = item.name.clone();
                                                                let current_p = webdav_current_path.clone();
                                                                let nav_click = on_webdav_navigate.clone();
                                                                let play_click = on_webdav_play.clone();
                                                                rsx! {
                                                                    div {
                                                                        class: "flex items-center p-1 rounded hover:bg-gray-600 cursor-pointer text-sm",
                                                                        onclick: move |_| {
                                                                            if is_dir {
                                                                                let mut path = current_p.clone();
                                                                                if !path.ends_with('/') {
                                                                                    path.push('/');
                                                                                }
                                                                                path.push_str(&item_name);
                                                                                nav_click.call(path);
                                                                            } else {
                                                                                play_click.call(item_clone.clone());
                                                                            }
                                                                        },
                                                                        span { class: "mr-2 text-xs",
                                                                            if is_dir {
                                                                                "📁"
                                                                            } else {
                                                                                "🎵"
                                                                            }
                                                                        }
                                                                        span { class: "truncate flex-1", "{item.name}" }
                                                                    }
                                                                }
                                                            })
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn PlaylistTracks(
    playlist: Playlist,
    current_track: Option<TrackStub>,
    on_track_select: EventHandler<TrackStub>,
) -> Element {
    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4",

            h3 { class: "text-lg font-bold mb-4", "🎶 Tracks" }

            div { class: "space-y-2 max-h-96 overflow-y-auto",
                {

                    playlist
                        .tracks
                        .iter()
                        .enumerate()
                        .map(|(idx, track)| {
                            let track_clone = track.clone();
                            let is_current = current_track
                                .as_ref()
                                .map(|t| t.id == track.id)
                                .unwrap_or(false);
                            let class_str = if is_current {
                                "w-full text-left px-3 py-2 rounded bg-blue-600 hover:bg-blue-700 text-sm"
                            } else {
                                "w-full text-left px-3 py-2 rounded bg-gray-700 hover:bg-gray-600 text-sm"
                            };
                            rsx! {
                                button {
                                    key: "{idx}",
                                    class: class_str,
                                    onclick: move |_| on_track_select.call(track_clone.clone()),

                
                                    div { class: "font-semibold truncate", "{track.title}" }
                                    p { class: "text-xs text-gray-300 truncate", "{track.artist}" }
                                    p { class: "text-xs text-gray-400", "{format_duration(track.duration)}" }
                                }
                            }
                        })
                }
            }
        }
    }
}

#[component]
fn PlaylistManagerModal(
    on_close: EventHandler<()>,
    on_add_playlist: EventHandler<String>,
    on_load_files: EventHandler<()>,
) -> Element {
    let mut playlist_name = use_signal(|| String::new());

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-96 shadow-xl",
                onclick: move |e| e.stop_propagation(),

                h2 { class: "text-2xl font-bold mb-4", "Create New Playlist" }

                input {
                    class: "w-full px-4 py-2 rounded bg-gray-700 border border-gray-600 mb-4 text-white",
                    placeholder: "Playlist name...",
                    value: playlist_name(),
                    oninput: move |e| {
                        *playlist_name.write() = e.value();
                    },
                }

                div { class: "flex gap-4 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 rounded disabled:opacity-50",
                        disabled: playlist_name().is_empty(),
                        onclick: move |_| {
                            on_add_playlist.call(playlist_name());
                        },
                        "Create"
                    }
                }
            }
        }
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

// Encode binary data to base64 for image display
fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b1 = data[i];
        let b2 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b3 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        let n = ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);

        result.push(CHARSET[((n >> 18) & 63) as usize] as char);
        result.push(CHARSET[((n >> 12) & 63) as usize] as char);

        if i + 1 < data.len() {
            result.push(CHARSET[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(CHARSET[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

// Scan directory for music files
pub fn scan_music_directory(path: &str) -> Result<Vec<TrackStub>, Box<dyn std::error::Error>> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if AUDIO_FORMATS.contains(&ext_lower.as_str()) {
                let track_stub = match crate::metadata::TrackMetadata::from_file(path) {
                    Ok(track) => TrackStub::from(track),
                    Err(_) => TrackStub {
                        id: Uuid::new_v4().to_string(),
                        path: path.to_string_lossy().to_string(),
                        title: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Unknown".to_string()),
                        artist: "Unknown Artist".to_string(),
                        album: "Unknown Album".to_string(),
                        duration: Duration::from_secs(0),
                    },
                };
                tracks.push(track_stub);
            }
        }
    }

    Ok(tracks)
}

// Save all playlists to a directory
pub fn save_all_playlists(
    playlists: &[Playlist],
    dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;

    for (_idx, playlist) in playlists.iter().enumerate() {
        let filename = format!("{}/{}.json", dir, playlist.id);
        playlist.save_to_file(&filename)?;
    }

    Ok(())
}

// Load all playlists from a directory
pub fn load_all_playlists(dir: &str) -> Result<Vec<Playlist>, Box<dyn std::error::Error>> {
    Playlist::load_multiple_from_dir(dir)
}

#[component]
fn DirectoryBrowserModal(
    current_directory: String,
    on_close: EventHandler<()>,
    on_load_directory: EventHandler<String>,
) -> Element {
    let mut selected_path = use_signal(|| current_directory.clone());
    let mut is_loading = use_signal(|| false);

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-full max-w-2xl shadow-xl",
                onclick: move |e| e.stop_propagation(),

                h2 { class: "text-2xl font-bold mb-4", "📁 Select Music Directory" }

                div { class: "bg-gray-700 rounded p-3 mb-4 text-sm break-all min-h-12 flex items-center",
                    if selected_path().is_empty() {
                        "No directory selected"
                    } else {
                        "{selected_path()}"
                    }
                }

                div { class: "text-xs text-gray-400 p-3 bg-gray-900 rounded mb-4",
                    "Supported formats: MP3, WAV, FLAC, OGG, M4A"
                }

                div { class: "flex gap-4 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded disabled:opacity-50",
                        disabled: is_loading(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded disabled:opacity-50",
                        disabled: is_loading(),
                        onclick: move |_| {
                            *is_loading.write() = true;
                            let handler = on_load_directory.clone();
                            spawn(async move {
                                if let Some(path) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    if let Some(path_str) = path.path().to_str() {
                                        *selected_path.write() = path_str.to_string();
                                        handler.call(path_str.to_string());
                                    }
                                }
                                *is_loading.write() = false;
                            });
                        },
                        if is_loading() {
                            "Loading..."
                        } else {
                            "📂 Browse Folder"
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-green-600 hover:bg-green-700 rounded disabled:opacity-50",
                        disabled: selected_path().is_empty() || is_loading(),
                        onclick: move |_| on_load_directory.call(selected_path()),
                        "✓ Load Music"
                    }
                }
            }
        }
    }
}

#[component]
fn WebDAVConfigListModal(
    configs: Vec<WebDAVConfig>,
    current_config: Option<usize>,
    on_close: EventHandler<()>,
    on_add_config: EventHandler<()>,
    on_edit_config: EventHandler<usize>,
    on_delete_config: EventHandler<usize>,
    on_select_config: EventHandler<usize>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-full max-w-2xl shadow-xl",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex justify-between items-center mb-4",
                    h2 { class: "text-2xl font-bold", "☁️ WebDAV Servers" }
                    button {
                        class: "text-gray-400 hover:text-white text-2xl",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                if configs.is_empty() {
                    div { class: "text-center py-8 text-gray-400", "No WebDAV servers configured yet" }
                } else {
                    div { class: "space-y-2 max-h-96 overflow-y-auto mb-4",
                        for (idx , config) in configs.iter().enumerate() {
                            div {
                                class: "flex items-center justify-between p-3 rounded",
                                class: if Some(idx) == current_config { "bg-blue-600" } else { "bg-gray-700" },

                                div {
                                    class: "flex-1 cursor-pointer",
                                    onclick: move |_| on_select_config.call(idx),

                                    div { class: "font-semibold", "{config.name}" }
                                    p { class: "text-xs text-gray-300 truncate", "{config.url}" }
                                    div { class: "text-xs mt-1",
                                        if config.enabled {
                                            span { class: "text-green-400", "✓ Enabled" }
                                        } else {
                                            span { class: "text-gray-400", "○ Disabled" }
                                        }
                                    }
                                }

                                div { class: "flex gap-2",
                                    button {
                                        class: "px-3 py-1 bg-blue-500 hover:bg-blue-600 rounded text-sm",
                                        onclick: move |_| on_edit_config.call(idx),
                                        "✎ Edit"
                                    }
                                    button {
                                        class: "px-3 py-1 bg-red-500 hover:bg-red-600 rounded text-sm",
                                        onclick: move |_| on_delete_config.call(idx),
                                        "🗑 Delete"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "flex gap-4 justify-between",
                    button {
                        class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                    button {
                        class: "px-4 py-2 bg-green-600 hover:bg-green-700 rounded",
                        onclick: move |_| on_add_config.call(()),
                        "+ Add Server"
                    }
                }
            }
        }
    }
}

#[component]
fn WebDAVConfigModal(
    config: WebDAVConfig,
    on_close: EventHandler<()>,
    on_save_config: EventHandler<WebDAVConfig>,
) -> Element {
    let mut name = use_signal(|| config.name.clone());
    let mut url = use_signal(|| config.url.clone());
    let mut username = use_signal(|| config.username.clone());
    let mut password = use_signal(|| config.password.clone());
    let mut enabled = use_signal(|| config.enabled);
    let mut test_status = use_signal(|| Option::<Result<bool, String>>::None);
    let mut is_testing = use_signal(|| false);

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-full max-w-2xl shadow-xl",
                onclick: move |e| e.stop_propagation(),

                h2 { class: "text-2xl font-bold mb-4", "Add WebDAV Server" }

                div { class: "space-y-4 mb-4",

                    div {
                        label { class: "block text-sm font-semibold mb-2", "Server Name" }
                        input {
                            class: "w-full px-4 py-2 rounded bg-gray-700 border border-gray-600 text-white",
                            placeholder: "e.g., Nextcloud Work, Aliyun Music",
                            value: name(),
                            oninput: move |e| *name.write() = e.value(),
                        }
                    }

                    div {
                        label { class: "block text-sm font-semibold mb-2", "Server URL" }
                        input {
                            class: "w-full px-4 py-2 rounded bg-gray-700 border border-gray-600 text-white",
                            placeholder: "https://nextcloud.example.com/remote.php/dav/files/username/",
                            value: url(),
                            oninput: move |e| *url.write() = e.value(),
                        }
                    }

                    div {
                        label { class: "block text-sm font-semibold mb-2", "Username" }
                        input {
                            class: "w-full px-4 py-2 rounded bg-gray-700 border border-gray-600 text-white",
                            placeholder: "Your username",
                            value: username(),
                            oninput: move |e| *username.write() = e.value(),
                        }
                    }

                    div {
                        label { class: "block text-sm font-semibold mb-2", "Password" }
                        input {
                            r#type: "password",
                            class: "w-full px-4 py-2 rounded bg-gray-700 border border-gray-600 text-white",
                            placeholder: "Your password",
                            value: password(),
                            oninput: move |e| *password.write() = e.value(),
                        }
                    }

                    div { class: "flex items-center gap-2",
                        input {
                            r#type: "checkbox",
                            id: "webdav-enabled",
                            checked: enabled(),
                            onchange: move |e| *enabled.write() = e.checked(),
                        }
                        label {
                            r#for: "webdav-enabled",
                            class: "text-sm font-semibold",
                            "Enable This Server"
                        }
                    }

                    div { class: "flex items-center gap-3 pt-2",
                        button {
                            class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded disabled:opacity-50",
                            disabled: url().is_empty() || is_testing(),
                            onclick: move |_| {
                                *is_testing.write() = true;
                                *test_status.write() = None;

                                let test_url = url().clone();
                                let test_username = username().clone();
                                let test_password = password().clone();

                                spawn(async move {
                                    let result = test_webdav_connection(
                                            &test_url,
                                            &test_username,
                                            &test_password,
                                        )
                                        .await;
                                    *test_status.write() = Some(result);
                                    *is_testing.write() = false;
                                });
                            },
                            if is_testing() {
                                "🔄 Testing..."
                            } else {
                                "🧪 Test Connection"
                            }
                        }

                        if let Some(Ok(_)) = test_status() {
                            span { class: "text-green-400 font-semibold text-lg", "OK Available" }
                        } else if let Some(Err(error_msg)) = test_status() {
                            span { class: "text-red-400 font-semibold text-lg", "FAIL Unavailable" }
                            div { class: "text-red-300 text-sm mt-1", "{error_msg}" }
                        }
                    }
                }

                div { class: "text-xs text-gray-400 p-3 bg-gray-900 rounded mb-4",
                    "Configure WebDAV servers (Nextcloud, Aliyun, etc.) to browse and access music from the cloud."
                }

                div { class: "flex gap-4 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-green-600 hover:bg-green-700 rounded disabled:opacity-50",
                        disabled: name().is_empty() || url().is_empty(),
                        onclick: move |_| {
                            on_save_config
                                .call(WebDAVConfig {
                                    id: config.id.clone(),
                                    name: name(),
                                    url: url(),
                                    username: username(),
                                    password: password(),
                                    enabled: enabled(),
                                });
                        },
                        "✓ Add Server"
                    }
                }
            }
        }
    }
}

// Test WebDAV connection availability
async fn test_webdav_connection(url: &str, username: &str, password: &str) -> Result<bool, String> {
    use base64::{engine::general_purpose, Engine as _};
    
    // Validate URL format
    let parsed_url = match reqwest::Url::parse(url) {
        Ok(u) => u,
        Err(e) => return Err(format!("URL格式错误: {}", e)),
    };
    
    // Check if URL has proper scheme
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err("URL必须以 http:// 或 https:// 开头".to_string());
    }
    
    // Prepare authorization header
    let auth_str = format!("{}:{}", username, password);
    let encoded = general_purpose::STANDARD.encode(auth_str.as_bytes());
    let auth_header = format!("Basic {}", encoded);

    // Try to make a PROPFIND request to test connection
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;
    
    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;
    
    let result = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), url)
        .header("Authorization", &auth_header)
        .header("Depth", "0")
        .header("Content-Type", "application/xml; charset=\"utf-8\"")
        .body(propfind_body.to_string())
        .send()
        .await;

    match result {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(true)
            } else if status.as_u16() == 401 {
                // 401 means auth required, but server exists
                Ok(true)
            } else if status.as_u16() == 405 {
                // 405 Method Not Allowed - PROPFIND not allowed, but server exists
                Ok(true)
            } else if status.as_u16() == 404 {
                Err(format!("服务器连接成功，但路径不存在 (HTTP 404)"))
            } else {
                Err(format!("服务器返回错误 (HTTP {})", status.as_u16()))
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Err("连接超时，请检查URL是否正确".to_string())
            } else if e.is_connect() {
                Err("无法连接到服务器，请检查URL和网络连接".to_string())
            } else {
                Err(format!("连接失败: {}", e))
            }
        }
    }
}

// Load WebDAV configs from disk
fn load_webdav_configs() -> Result<Vec<WebDAVConfig>, Box<dyn std::error::Error>> {
    let config_dir = get_config_dir()?;
    let config_file = std::path::PathBuf::from(&config_dir).join("webdav_configs.json");
    
    if config_file.exists() {
        let content = std::fs::read_to_string(&config_file)?;
        let configs: Vec<WebDAVConfig> = serde_json::from_str(&content)?;
        Ok(configs)
    } else {
        Ok(Vec::new())
    }
}

// Save WebDAV configs to disk
fn save_webdav_configs(configs: &[WebDAVConfig]) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    
    let config_file = std::path::PathBuf::from(&config_dir).join("webdav_configs.json");
    let json = serde_json::to_string_pretty(configs)?;
    std::fs::write(config_file, json)?;
    
    Ok(())
}

// Get config directory
fn get_config_dir() -> Result<String, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    Ok(format!("{}/.dioxus_music", home))
}

#[component]
fn WebDAVBrowserModal(
    config: WebDAVConfig,
    on_close: EventHandler<()>,
    on_import_folder: EventHandler<Vec<Track>>,
) -> Element {
    let config = use_signal(|| config.clone());
    let mut current_path = use_signal(|| "/".to_string());
    let mut items = use_signal(|| Vec::new());
    let mut selected_items = use_signal(|| Vec::new());
    let mut is_loading = use_signal(|| false);
    let mut error_msg = use_signal(|| Option::<String>::None);

    // Load root directory on mount
    use_effect(move || {
        let cfg = config();
        let current = current_path();
        *is_loading.write() = true;
        
        spawn(async move {
            match load_webdav_folder(&cfg, &current).await {
                Ok(folder_items) => {
                    *items.write() = folder_items;
                    *error_msg.write() = None;
                }
                Err(e) => {
                    *error_msg.write() = Some(format!("加载失败: {}", e));
                }
            }
            *is_loading.write() = false;
        });
    });

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-full max-w-4xl max-h-96 shadow-xl overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                div { class: "flex justify-between items-center mb-4",
                    h2 { class: "text-2xl font-bold", "🌐 Browse {config().name}" }
                    button {
                        class: "text-gray-400 hover:text-white text-2xl",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                div { class: "bg-gray-700 rounded p-3 mb-4 text-sm break-all", "{current_path()}" }

                if let Some(err) = error_msg() {
                    div { class: "bg-red-900 text-red-200 p-3 rounded mb-4 text-sm",
                        "{err}"
                    }
                }

                if is_loading() {
                    div { class: "text-center py-8 text-gray-400", "🔄 Loading..." }
                } else if items().is_empty() {
                    div { class: "text-center py-8 text-gray-400", "No items found" }
                } else {
                    div { class: "space-y-1 mb-4 max-h-48 overflow-y-auto",
                        for (idx , item) in items().into_iter().enumerate() {
                            div {
                                key: "{idx}",
                                class: "flex items-center justify-between p-2 rounded hover:bg-gray-600 cursor-pointer",

                                div {
                                    class: "flex-1",
                                    onclick: move |_| {
                                        if item.is_dir {
                                            let mut path = current_path();
                                            if !path.ends_with('/') {
                                                path.push('/');
                                            }
                                            path.push_str(&item.name);

                                            let cfg = config();
                                            *current_path.write() = path.clone();
                                            *is_loading.write() = true;

                                            spawn(async move {
                                                match load_webdav_folder(&cfg, &path).await {
                                                    Ok(folder_items) => {
                                                        *items.write() = folder_items;
                                                        *error_msg.write() = None;
                                                    }
                                                    Err(e) => {
                                                        *error_msg.write() = Some(format!("加载失败: {}", e));
                                                    }
                                                }
                                                *is_loading.write() = false;
                                            });
                                        }
                                    },

                                    span { class: "text-lg mr-2",
                                        if item.is_dir {
                                            "📁"
                                        } else {
                                            "🎵"
                                        }
                                    }
                                    span { "{item.name}" }
                                    if !item.is_dir {
                                        span { class: "text-xs text-gray-400 ml-2",
                                            "({item.size} bytes)"
                                        }
                                    }
                                }

                                if !item.is_dir {
                                    input {
                                        r#type: "checkbox",
                                        checked: selected_items().contains(&item.path),
                                        onchange: move |e| {
                                            let mut sel = selected_items();
                                            if e.checked() {
                                                sel.push(item.path.clone());
                                            } else {
                                                sel.retain(|p| p != &item.path);
                                            }
                                            *selected_items.write() = sel;
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "flex gap-4 justify-between",
                    div { class: "flex gap-2",
                        if current_path() != "/" {
                            button {
                                class: "px-3 py-2 bg-gray-600 hover:bg-gray-700 rounded text-sm",
                                onclick: move |_| {
                                    let mut path = current_path();
                                    if path.ends_with('/') {
                                        path.pop();
                                    }
                                    if let Some(pos) = path.rfind('/') {
                                        path.truncate(pos + 1);
                                    } else {
                                        path = "/".to_string();
                                    }

                                    let cfg = config();
                                    *current_path.write() = path.clone();
                                    *is_loading.write() = true;

                                    spawn(async move {
                                        match load_webdav_folder(&cfg, &path).await {
                                            Ok(folder_items) => {
                                                *items.write() = folder_items;
                                                *error_msg.write() = None;
                                            }
                                            Err(e) => {
                                                *error_msg.write() = Some(format!("加载失败: {}", e));
                                            }
                                        }
                                        *is_loading.write() = false;
                                    });
                                },
                                "⬆ Back"
                            }
                        }
                    }

                    div { class: "flex gap-2",
                        button {
                            class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded",
                            onclick: move |_| on_close.call(()),
                            "Close"
                        }
                        button {
                            class: "px-4 py-2 bg-green-600 hover:bg-green-700 rounded disabled:opacity-50",
                            disabled: selected_items().is_empty() || is_loading(),
                            onclick: move |_| {
                                let selected = selected_items();
                                if !selected.is_empty() {
                                    let cfg = config();
                                    let selected_clone = selected.clone();

                                    spawn(async move {
                                        match download_and_import_webdav_files(&cfg, &selected_clone).await {
                                            Ok(tracks) => {
                                                on_import_folder.call(tracks);
                                            }
                                            // Error handling
                                            Err(_e) => {}
                                        }
                                    });
                                }
                            },
                            "✓ Import ({selected_items().len()})"
                        }
                    }
                }
            }
        }
    }
}

// Load WebDAV folder items
async fn load_webdav_folder(config: &WebDAVConfig, path: &str) -> Result<Vec<webdav::WebDAVItem>, Box<dyn std::error::Error>> {
    use webdav::WebDAVClient;
    
    let client = WebDAVClient::new(config.url.clone())
        .with_auth(config.username.clone(), config.password.clone());
    
    let items = client.list_items(path).await?;
    
    // Filter to show only folders and audio files
    let filtered: Vec<webdav::WebDAVItem> = items
        .into_iter()
        .filter(|item| item.is_dir || is_audio_file(&item.name))
        .collect();
    
    Ok(filtered)
}

// Check if file is an audio file
fn is_audio_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    AUDIO_FORMATS.iter().any(|fmt| lower.ends_with(&format!(".{}", fmt)))
}

#[component]
fn WebDAVSidebar(
    config: WebDAVConfig,
    current_path: String,
    items: Vec<webdav::WebDAVItem>,
    is_loading: bool,
    error_msg: Option<String>,
    on_navigate: EventHandler<String>,
    on_play_track: EventHandler<webdav::WebDAVItem>,
    on_close: EventHandler<()>,
) -> Element {
    let up_path = current_path.clone();
    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4 h-full flex flex-col",
            div { class: "flex justify-between items-center mb-4",
                h3 { class: "text-lg font-bold truncate", "☁️ {config.name}" }
                button {
                    class: "text-gray-400 hover:text-white",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            // Path breadcrumb/navigation
            div { class: "flex gap-2 mb-2 text-sm",
                if current_path != "/" {
                    button {
                        class: "px-2 py-1 bg-gray-700 hover:bg-gray-600 rounded",
                        onclick: move |_| {
                            let mut path = up_path.clone();
                            if path.ends_with('/') {
                                path.pop();
                            }
                            if let Some(pos) = path.rfind('/') {
                                path.truncate(pos + 1);
                            } else {
                                path = "/".to_string();
                            }
                            on_navigate.call(path);
                        },
                        "⬆ .."
                    }
                }
                div { class: "px-2 py-1 bg-gray-700 rounded flex-1 truncate font-mono text-xs",
                    "{current_path}"
                }
            }

            if let Some(err) = error_msg {
                div { class: "bg-red-900 text-red-200 p-2 rounded mb-2 text-xs", "{err}" }
            }

            div { class: "flex-1 overflow-y-auto space-y-1",
                if is_loading {
                    div { class: "text-center py-4 text-gray-400", "🔄 Loading..." }
                } else if items.is_empty() {
                    div { class: "text-center py-4 text-gray-400", "Empty folder" }
                } else {
                    {

                        items

                            .iter()
                            .enumerate()
                            .map(|(idx, item)| {
                                let item_click = item.clone();
                                let path_click = current_path.clone();
                                let nav_click = on_navigate.clone();
                                let play_click = on_play_track.clone();
                                rsx! {
                                    div {
                                        key: "{idx}",
                                        class: "flex items-center p-2 rounded hover:bg-gray-700 cursor-pointer group",
                                        onclick: move |_| {
                                            if item_click.is_dir {
                                                let mut path = path_click.clone();
                                                if !path.ends_with('/') {
                                                    path.push('/');
                                                }
                                                path.push_str(&item_click.name);
                                                nav_click.call(path);
                                            } else {
                                                play_click.call(item_click.clone());
                                            }
                                        },

                

                                        span { class: "mr-2",
                                            if item.is_dir {
                                                "📁"
                                            } else {
                                                "🎵"
                                            }
                                        }
                
                                        div { class: "flex-1 min-w-0",
                                            div { class: "truncate text-sm", "{item.name}" }
                                            if !item.is_dir {
                                                div { class: "text-xs text-gray-500 truncate", "{item.size / 1024} KB • {item.modified}" }
                                            }
                                        }
                                    }
                                }
                            })
                    }
                }
            }
        }
    }
}

// Create placeholder Track for WebDAV files without downloading (for adding to playlist)
async fn create_webdav_placeholder_tracks(
    config: &WebDAVConfig,
    file_paths: &[String],
) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let mut tracks = Vec::new();
    
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).map_err(|_| "Invalid username")?;
        if !config.password.is_empty() {
            base_url.set_password(Some(&config.password)).map_err(|_| "Invalid password")?;
        }
    }
    
    for path_str in file_paths {
        let full_url = if path_str.starts_with("http") {
            path_str.to_string()
        } else {
            // 构建包含认证信息的完整 URL
            // config.url = http://192.168.2.5:5244/dav/tianyi
            // 完整 URL = http://username:password@192.168.2.5:5244/dav/tianyi/音乐/xxx.flac
            let base = config.url.trim_end_matches('/');

            // 找到协议后的位置
            let proto_end = base.find("://").map(|p| p + 3).unwrap_or(0);

            // 找到路径开始位置
            let path_start = base[proto_end..].find('/').map(|p| proto_end + p).unwrap_or(base.len());

            let host_port = &base[proto_end..path_start];
            let base_path = &base[path_start..];

            let auth_part = if !config.username.is_empty() {
                format!("{}:{}@", config.username, config.password)
            } else {
                String::new()
            };

            format!("{}{}{}{}{}{}", &base[..proto_end], auth_part, host_port, base_path,
                if path_str.starts_with('/') { "" } else { "/" },
                path_str)
        };

        eprintln!("[WebDAV DEBUG] full_url='{}'", full_url);
        
        let filename = path_str.split('/').last().unwrap_or("Unknown");
        let decoded_filename = match urlencoding::decode(filename) {
            Ok(cow) => cow.into_owned(),
            Err(_) => filename.to_string(),
        };
        let title = std::path::Path::new(&decoded_filename)
           .file_stem()
           .and_then(|s| s.to_str())
           .unwrap_or(&decoded_filename)
           .to_string();
        
        let track = Track {
            id: uuid::Uuid::new_v4().to_string(),
            path: full_url,
            title: title,
            artist: "Cloud Stream".to_string(), 
            album: "WebDAV".to_string(),
            duration: std::time::Duration::from_secs(0),
            cover: None,
        };
        tracks.push(track);
    }
    
    Ok(tracks)
}

// Import WebDAV files as streams (downloads to get metadata)
async fn download_and_import_webdav_files(
    config: &WebDAVConfig,
    file_paths: &[String],
) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let mut tracks = Vec::new();
    
    let client = reqwest::Client::new();
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).map_err(|_| "Invalid username")?;
        if !config.password.is_empty() {
            base_url.set_password(Some(&config.password)).map_err(|_| "Invalid password")?;
        }
    }
    
    for path_str in file_paths {
        let full_url = if path_str.starts_with("http") {
            let mut u = reqwest::Url::parse(path_str)?;
            if !base_url.username().is_empty() {
                u.set_username(base_url.username()).ok();
                u.set_password(base_url.password()).ok();
            }
            u.to_string()
        } else {
            base_url.join(path_str)?.to_string()
        };
        
        let filename = path_str.split('/').last().unwrap_or("Unknown");
        let decoded_filename = match urlencoding::decode(filename) {
            Ok(cow) => cow.into_owned(),
            Err(_) => filename.to_string(),
        };
        let title = std::path::Path::new(&decoded_filename)
           .file_stem()
           .and_then(|s| s.to_str())
           .unwrap_or(&decoded_filename)
           .to_string();
        
        // Try to get duration from metadata
        let mut duration = std::time::Duration::from_secs(0);
        
        // Download file to temp location to read metadata
        let temp_dir = std::env::temp_dir();
        let temp_filename = format!("dioxusmusic_{}", uuid::Uuid::new_v4());
        let temp_path = temp_dir.join(&temp_filename);
        
        match client.get(&full_url)
            .basic_auth(&config.username, Some(&config.password))
            .send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.bytes().await {
                        Ok(bytes) => {
                            if let Ok(_) = std::fs::write(&temp_path, &bytes) {
                                // Try to read metadata from downloaded file
                                if let Ok(d) = mp3_duration::from_path(&temp_path) {
                                    duration = d;
                                }
                                // Clean up temp file
                                let _ = std::fs::remove_file(&temp_path);
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            Err(_) => {}
        }
        
        let track = Track {
            id: uuid::Uuid::new_v4().to_string(),
            path: full_url,
            title: title,
            artist: "Cloud Stream".to_string(), 
            album: "WebDAV".to_string(),
            duration: duration,
            cover: None,
        };
        tracks.push(track);
    }
    
    Ok(tracks)
}

// Fetch metadata for a single WebDAV file on-demand (when playing)
async fn fetch_webdav_track_metadata(
    config: &WebDAVConfig,
    path: &str,
) -> Result<std::time::Duration, Box<dyn std::error::Error>> {
    let mut duration = std::time::Duration::from_secs(0);
    
    let client = reqwest::Client::new();
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).ok();
        base_url.set_password(Some(&config.password)).ok();
    }
    
    let full_url = if path.starts_with("http") {
        let mut u = reqwest::Url::parse(path)?;
        if !base_url.username().is_empty() {
            u.set_username(base_url.username()).ok();
            u.set_password(base_url.password()).ok();
        }
        u.to_string()
    } else {
        // path 是相对于 base_url 的路径
        // base_url 已经包含配置中的子目录路径
        base_url.join(path)?.to_string()
    };
    
    let temp_dir = std::env::temp_dir();
    let temp_filename = format!("dioxusmusic_{}", uuid::Uuid::new_v4());
    let temp_path = temp_dir.join(&temp_filename);
    
    match client.get(&full_url)
        .basic_auth(&config.username, Some(&config.password))
        .send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(bytes) => {
                        if let Ok(_) = std::fs::write(&temp_path, &bytes) {
                            if let Ok(d) = mp3_duration::from_path(&temp_path) {
                                duration = d;
                            }
                            let _ = std::fs::remove_file(&temp_path);
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        Err(_) => {}
    }
    
    Ok(duration)
 }

 // Play WebDAV track with on-demand metadata fetching
 async fn play_webdav_track(
     config: &WebDAVConfig,
     track: &mut Track,
 ) -> Result<(), Box<dyn std::error::Error>> {
     // Fetch metadata on-demand (only when playing)
     if track.duration.as_secs() == 0 {
         if let Ok(duration) = fetch_webdav_track_metadata(config, &track.path).await {
             track.duration = duration;
         }
     }
     Ok(())
 }

