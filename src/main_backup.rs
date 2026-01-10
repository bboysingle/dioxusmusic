mod player;
mod playlist;
mod metadata;
mod webdav;

use dioxus::prelude::*;
use player::{MusicPlayer, PlayerState};
use playlist::Playlist;
use metadata::TrackMetadata;
use std::time::Duration;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

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
pub struct WebDAVConfig {
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
    let mut current_track = use_signal(|| None::<Track>);
    let mut current_time = use_signal(|| Duration::from_secs(0));
    let mut volume = use_signal(|| 0.7);
    let mut playlists = use_signal(|| vec![Playlist::new("My Playlist".to_string())]);
    let mut current_playlist = use_signal(|| 0);
    let mut show_playlist_manager = use_signal(|| false);
    let mut show_directory_browser = use_signal(|| false);
    let mut show_webdav_config = use_signal(|| false);
    let mut webdav_config = use_signal(|| WebDAVConfig {
        url: String::new(),
        username: String::new(),
        password: String::new(),
        enabled: false,
    });
    let mut current_directory = use_signal(|| String::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_string())));
    
    // Create a static-like player reference stored in component state
    // This will be created once and persist for the lifetime of the app
    let player_ref = use_signal(|| MusicPlayer::new().ok());
    
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
                            onclick: move |_| *show_webdav_config.write() = true,
                            "☁️ WebDAV Config"
                        }
                    }
                }
            }

            main { class: "max-w-7xl mx-auto p-6",

                div { class: "grid grid-cols-3 gap-6",

                    aside { class: "col-span-1",
                        PlaylistSidebar {
                            playlists: playlists(),
                            current_playlist: current_playlist(),
                            on_select: move |idx| {
                                *current_playlist.write() = idx;
                            },
                            on_add_playlist: move |_| {
                                *show_playlist_manager.write() = true;
                            },
                        }
                    }

                    section { class: "col-span-2",

                        NowPlayingCard { current_track: current_track() }

                        PlayerControls {
                            state: player_state(),
                            current_time: current_time(),
                            duration: current_track().as_ref().map(|t| t.duration),
                            volume: volume(),
                            on_play: move |_| {
                                // Read the current player value
                                if let Some(ref player) = *player_ref.read() {
                                    // If paused, resume from where we left off
                                    // Otherwise, play from the beginning
                                    if player_state() == PlayerState::Paused && player.is_paused() {
                                        let _ = player.resume();
                                    } else if let Some(track) = current_track() {
                                        let _ = player.play(std::path::Path::new(&track.path));
                                        let _ = player.set_volume(volume());
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
                                    let _ = player.stop();
                                }
                                *player_state.write() = PlayerState::Stopped;
                            },
                            on_seek: move |time| {
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
                                        if let Some(pos) = playlist.tracks.iter().position(|t| t.id == current.id) {
                                            if pos > 0 {
                                                let prev_track = playlist.tracks[pos - 1].clone();
                                                if let Some(ref player) = *player_ref.read() {
                                                    let _ = player.play(std::path::Path::new(&prev_track.path));
                                                    let _ = player.set_volume(volume());
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
                                        if let Some(pos) = playlist.tracks.iter().position(|t| t.id == current.id) {
                                            if pos < playlist.tracks.len() - 1 {
                                                let next_track = playlist.tracks[pos + 1].clone();
                                                if let Some(ref player) = *player_ref.read() {
                                                    let _ = player.play(std::path::Path::new(&next_track.path));
                                                    let _ = player.set_volume(volume());
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
                                on_track_select: move |track: Track| {
                                    if let Some(ref player) = *player_ref.read() {
                                        let _ = player.play(std::path::Path::new(&track.path));
                                        let _ = player.set_volume(volume());
                                    }
                                    *current_track.write() = Some(track);
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

            if show_webdav_config() {
                WebDAVConfigModal {
                    config: webdav_config(),
                    on_close: move |_| {
                        *show_webdav_config.write() = false;
                    },
                    on_save_config: move |config| {
                        *webdav_config.write() = config;
                        *show_webdav_config.write() = false;
                    },
                }
            }
        }
    }
}

#[component]
fn NowPlayingCard(current_track: Option<Track>) -> Element {
    let cover_img = current_track.as_ref().and_then(|track| {
        track.cover.as_ref().map(|cover_data| {
            let base64_cover = base64_encode(cover_data);
            format!("data:image/jpeg;base64,{}", base64_cover)
        })
    });

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-8 mb-6 text-center",

            if let Some(track) = current_track {
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
                h2 { class: "text-2xl font-bold mb-2", "{track.title}" }
                p { class: "text-gray-400 mb-1", "{track.artist}" }
                p { class: "text-gray-500 text-sm", "{track.album}" }
            } else {
                div { class: "w-48 h-48 mx-auto mb-4 rounded-lg shadow-lg bg-gray-700 flex items-center justify-center text-4xl",
                    "🎵"
                }
                p { class: "text-gray-400", "No track selected" }
            }
        }
    }
}

#[component]
fn PlayerControls(
    state: PlayerState,
    current_time: Duration,
    duration: Option<Duration>,
    volume: f32,
    on_play: EventHandler<()>,
    on_pause: EventHandler<()>,
    on_stop: EventHandler<()>,
    on_seek: EventHandler<Duration>,
    on_volume_change: EventHandler<f32>,
    on_previous: EventHandler<()>,
    on_next: EventHandler<()>,
) -> Element {
    let formatted_time = format_duration(current_time);
    let formatted_duration = duration.map(format_duration).unwrap_or_else(|| "0:00".to_string());
    let progress_percent = if let Some(d) = duration {
        if d.as_secs() > 0 {
            (current_time.as_secs_f64() / d.as_secs_f64() * 100.0) as i32
        } else {
            0
        }
    } else {
        0
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 mb-6",

            div { class: "mb-4",
                div { class: "w-full bg-gray-700 rounded-full h-2",
                    div {
                        class: "bg-blue-500 h-2 rounded-full",
                        style: "width: {progress_percent}%",
                    }
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
    on_select: EventHandler<usize>,
    on_add_playlist: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4",

            div { class: "flex justify-between items-center mb-4",
                h3 { class: "text-lg font-bold", "📋 Playlists" }
                button {
                    class: "px-3 py-1 bg-blue-500 hover:bg-blue-600 rounded text-sm",
                    onclick: move |_| on_add_playlist.call(()),
                    "+ New"
                }
            }

            div { class: "space-y-2 max-h-96 overflow-y-auto",
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
    }
}

#[component]
fn PlaylistTracks(
    playlist: Playlist,
    current_track: Option<Track>,
    on_track_select: EventHandler<Track>,
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
pub fn scan_music_directory(path: &str) -> Result<Vec<Track>, Box<dyn std::error::Error>> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if AUDIO_FORMATS.contains(&ext.to_lowercase().as_str()) {
                if let Ok(track) = TrackMetadata::from_file(path) {
                    tracks.push(track);
                }
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
fn WebDAVConfigModal(
    config: WebDAVConfig,
    on_close: EventHandler<()>,
    on_save_config: EventHandler<WebDAVConfig>,
) -> Element {
    let mut url = use_signal(|| config.url.clone());
    let mut username = use_signal(|| config.username.clone());
    let mut password = use_signal(|| config.password.clone());
    let mut enabled = use_signal(|| config.enabled);
    let mut test_status = use_signal(|| Option::<bool>::None);
    let mut is_testing = use_signal(|| false);

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 w-full max-w-2xl shadow-xl",
                onclick: move |e| e.stop_propagation(),

                h2 { class: "text-2xl font-bold mb-4", "☁️ WebDAV Configuration" }

                div { class: "space-y-4 mb-4",

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
                            "Enable WebDAV"
                        }
                    }

                    div { class: "flex items-center gap-3",
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
                                    let result = test_webdav_connection(&test_url, &test_username, &test_password).await;
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

                        if let Some(is_available) = test_status() {
                            if is_available {
                                span { class: "text-green-400 font-semibold text-lg", "✓ Available" }
                            } else {
                                span { class: "text-red-400 font-semibold text-lg", "✗ Unavailable" }
                            }
                        }
                    }
                }

                div { class: "text-xs text-gray-400 p-3 bg-gray-900 rounded mb-4",
                    "Configure your WebDAV server (Nextcloud, Aliyun, etc.) to browse and download music from the cloud."
                }

                div { class: "flex gap-4 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-600 hover:bg-gray-700 rounded",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-purple-600 hover:bg-purple-700 rounded",
                        onclick: move |_| {
                            on_save_config
                                .call(WebDAVConfig {
                                    url: url(),
                                    username: username(),
                                    password: password(),
                                    enabled: enabled(),
                                });
                        },
                        "✓ Save Configuration"
                    }
                }
            }
        }
    }
}

// Test WebDAV connection availability
async fn test_webdav_connection(url: &str, username: &str, password: &str) -> bool {
    use base64::{engine::general_purpose, Engine as _};
    
    // Prepare authorization header
    let auth_str = format!("{}:{}", username, password);
    let encoded = general_purpose::STANDARD.encode(auth_str.as_bytes());
    let auth_header = format!("Basic {}", encoded);

    // Try to make a PROPFIND request to test connection
    let client = reqwest::Client::new();
    
    let result = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap_or(reqwest::Method::GET), url)
        .header("Authorization", &auth_header)
        .header("Depth", "0")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match result {
        Ok(response) => {
            // Check if status is 2xx (success) or 401 (auth required - but server exists)
            let status = response.status();
            status.is_success() || status.as_u16() == 401
        }
        Err(_) => false,
    }
}
