mod player;
mod playlist;
mod metadata;
mod webdav;
mod crypto;

use dioxus::prelude::*;
use player::{MusicPlayer, PlayerState};
use playlist::Playlist;
use metadata::TrackMetadata;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use uuid::Uuid;
use std::sync::{Arc, Mutex};

fn load_header_icon() -> Option<String> {
    std::fs::read("assets/rmusic.ico")
        .ok()
        .and_then(|data| {
            image::load_from_memory_with_format(&data, image::ImageFormat::Ico).ok()
        })
        .and_then(|image| {
            let rgba = image.to_rgba8();
            dioxus_desktop::tao::window::Icon::from_rgba(rgba.into_raw(), image.width(), image.height()).ok()
        })
        .and_then(|_| {
            let data = std::fs::read("assets/rmusic.ico").ok()?;
            let base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
            Some(format!("data:image/x-icon;base64,{}", base64))
        })
}

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
    pub encrypted_password: String,
    pub enabled: bool,
    #[serde(skip)]
    pub password: Option<String>,
}

impl WebDAVConfig {
    pub fn get_password(&self) -> Result<String, Box<dyn std::error::Error>> {
        // 先检查是否有旧格式的明文密码
        if let Some(ref pwd) = self.password {
            if !pwd.is_empty() {
                return Ok(pwd.clone());
            }
        }

        if self.encrypted_password.is_empty() {
            return Ok(String::new());
        }

        let master_password = crypto::get_master_password()?;

        match crypto::decrypt_password(&self.encrypted_password, &master_password) {
            Ok(p) => Ok(p),
            Err(_) => {
                // 解密失败，可能是跨平台迁移
                eprintln!("[WebDAV] 解密失败，密码可能是在其他平台加密的");
                Err("Password decryption failed. The password may have been encrypted on a different platform. Please re-enter the password.".into())
            }
        }
    }

    pub fn set_password(&mut self, password: &str) -> Result<(), Box<dyn std::error::Error>> {
        if password.is_empty() {
            self.encrypted_password = String::new();
            self.password = None;
            return Ok(());
        }
        let master_password = crypto::get_master_password()?;
        self.encrypted_password = crypto::encrypt_password(password, &master_password)?;
        self.password = None;
        Ok(())
    }
}

fn main() {
    use dioxus::prelude::VirtualDom;
    use dioxus_desktop::{Config, WindowBuilder};

    let icon_path = std::path::Path::new("assets/rmusic.ico");

    eprintln!("[DEBUG] Icon exists: {}", icon_path.exists());

    let icon = std::fs::read(icon_path)
        .ok()
        .and_then(|data| {
            eprintln!("[DEBUG] Icon file size: {} bytes", data.len());

            let images = image::load_from_memory_with_format(&data, image::ImageFormat::Ico).ok()?;

            eprintln!("[DEBUG] Image dimensions: {}x{}", images.width(), images.height());

            let rgba = images.to_rgba8();
            let (width, height) = (images.width(), images.height());

            eprintln!("[DEBUG] Creating icon {}x{}", width, height);

            dioxus_desktop::tao::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
        });

    if icon.is_none() {
        eprintln!("[DEBUG] Failed to load icon");
    } else {
        eprintln!("[DEBUG] Icon loaded successfully");
    }

    let mut window = WindowBuilder::new()
        .with_title("Dioxus Music Player")
        .with_inner_size(dioxus_desktop::tao::dpi::LogicalSize::new(1200.0, 800.0));

    if let Some(icon) = icon {
        window = window.with_window_icon(Some(icon));
        eprintln!("[DEBUG] Icon set on window");
    }

    let cfg = Config::default()
        .with_window(window)
        .with_custom_head(String::from(r#"
            <style>
                * { margin: 0; padding: 0; box-sizing: border-box; }
                html, body {
                    background-color: #0f1116;
                    color: #e5e7eb;
                    font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
                    margin: 0;
                    padding: 0;
                    height: 100%;
                    overflow: hidden;
                }
                
                /* Root container */
                #root {
                    height: 100%;
                    display: flex;
                    flex-direction: column;
                }
                
                .min-h-screen { min-height: 100vh; }
                .bg-gray-800 { background-color: #1f2937; }
                .bg-gray-900 { background-color: #111827; }
                .bg-gray-700 { background-color: #374151; }
                .bg-gradient-to-b { background: linear-gradient(180deg, #1f2937 0%, #0f1116 100%); }
                .bg-gradient-to-r { background: linear-gradient(90deg, #3b82f6 0%, #8b5cf6 100%); }
                .text-white { color: #ffffff; }
                .text-gray-100 { color: #f3f4f6; }
                .text-gray-200 { color: #e5e7eb; }
                .text-gray-300 { color: #d1d5db; }
                .text-gray-400 { color: #9ca3af; }
                .text-gray-500 { color: #6b7280; }
                .text-blue-400 { color: #60a5fa; }
                .text-green-400 { color: #4ade80; }
                .text-red-400 { color: #f87171; }
                .text-yellow-400 { color: #fbbf24; }
                .p-6 { padding: 1.5rem; }
                .p-4 { padding: 1rem; }
                .p-3 { padding: 0.75rem; }
                .p-2 { padding: 0.5rem; }
                .m-4 { margin: 1rem; }
                .mb-6 { margin-bottom: 1.5rem; }
                .mb-4 { margin-bottom: 1rem; }
                .mb-3 { margin-bottom: 0.75rem; }
                .mb-2 { margin-bottom: 0.5rem; }
                .mt-4 { margin-top: 1rem; }
                .mt-2 { margin-top: 0.5rem; }
                .ml-2 { margin-left: 0.5rem; }
                .mr-2 { margin-right: 0.5rem; }
                .flex { display: flex; }
                .flex-col { flex-direction: column; }
                .items-center { align-items: center; }
                .items-start { align-items: flex-start; }
                .justify-center { justify-content: center; }
                .justify-between { justify-content: space-between; }
                .justify-end { justify-content: flex-end; }
                .gap-1 { gap: 0.25rem; }
                .gap-2 { gap: 0.5rem; }
                .gap-3 { gap: 0.75rem; }
                .gap-4 { gap: 1rem; }
                .gap-6 { gap: 1.5rem; }
                .grid { display: grid; }
                .grid-cols-3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }
                .col-span-1 { grid-column: span 1 / span 1; }
                .col-span-2 { grid-column: span 2 / span 2; }
                .rounded { border-radius: 0.25rem; }
                .rounded-full { border-radius: 9999px; }
                .rounded-lg { border-radius: 0.5rem; }
                .rounded-xl { border-radius: 0.75rem; }
                .rounded-2xl { border-radius: 1rem; }
                .shadow-lg { box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.3), 0 4px 6px -2px rgba(0, 0, 0, 0.2); }
                .shadow-md { box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -1px rgba(0, 0, 0, 0.2); }
                .shadow-xl { box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.2); }
                .shadow-inner { box-shadow: inset 0 2px 4px 0 rgba(0, 0, 0, 0.2); }
                .bg-blue-500 { background-color: #3b82f6; }
                .bg-blue-600 { background-color: #2563eb; }
                .bg-blue-700 { background-color: #1d4ed8; }
                .bg-green-500 { background-color: #22c55e; }
                .bg-green-600 { background-color: #16a34a; }
                .bg-red-500 { background-color: #ef4444; }
                .bg-red-600 { background-color: #dc2626; }
                .bg-yellow-500 { background-color: #eab308; }
                .bg-purple-500 { background-color: #a855f7; }
                .bg-purple-600 { background-color: #9333ea; }
                .bg-indigo-500 { background-color: #6366f1; }
                .bg-pink-500 { background-color: #ec4899; }
                .hover\:bg-blue-600:hover { background-color: #2563eb; }
                .hover\:bg-blue-700:hover { background-color: #1d4ed8; }
                .hover\:bg-green-600:hover { background-color: #16a34a; }
                .hover\:bg-red-600:hover { background-color: #dc2626; }
                .hover\:bg-blue-500:hover { background-color: #3b82f6; }
                .hover\:bg-gray-600:hover { background-color: #4b5563; }
                .hover\:bg-gray-700:hover { background-color: #374151; }
                .hover\:bg-purple-700:hover { background-color: #7c3aed; }
                .hover\:text-white:hover { color: #ffffff; }
                .text-sm { font-size: 0.875rem; }
                .text-xs { font-size: 0.75rem; }
                .text-base { font-size: 1rem; }
                .text-lg { font-size: 1.125rem; }
                .text-xl { font-size: 1.25rem; }
                .text-2xl { font-size: 1.5rem; }
                .text-3xl { font-size: 1.875rem; }
                .text-4xl { font-size: 2.25rem; }
                .text-5xl { font-size: 3rem; }
                .font-bold { font-weight: 700; }
                .font-semibold { font-weight: 600; }
                .font-medium { font-weight: 500; }
                .w-full { width: 100%; }
                .w-48 { width: 12rem; }
                .w-40 { width: 10rem; }
                .w-32 { width: 8rem; }
                .w-20 { width: 5rem; }
                .h-48 { height: 12rem; }
                .h-12 { height: 3rem; }
                .h-10 { height: 2.5rem; }
                .h-8 { height: 2rem; }
                .mx-auto { margin-left: auto; margin-right: auto; }
                .max-w-7xl { max-width: 80rem; }
                .max-w-4xl { max-width: 56rem; }
                .max-w-2xl { max-width: 42rem; }
                .max-w-md { max-width: 28rem; }
                .truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
                .space-y-2 > * + * { margin-top: 0.5rem; }
                .space-y-3 > * + * { margin-top: 0.75rem; }
                .space-y-4 > * + * { margin-top: 1rem; }
                .overflow-y-auto { overflow-y: auto; }
                .overflow-hidden { overflow: hidden; }
                .max-h-96 { max-height: 24rem; }
                .max-h-80 { max-height: 20rem; }
                .object-cover { object-fit: cover; }
                .fixed { position: fixed; }
                .absolute { position: absolute; }
                .relative { position: relative; }
                .inset-0 { top: 0; right: 0; bottom: 0; left: 0; }
                .bg-black { background-color: #000; }
                .bg-opacity-50 { background-color: rgba(0, 0, 0, 0.5); }
                .bg-opacity-70 { background-color: rgba(0, 0, 0, 0.7); }
                .flex-1 { flex: 1 1 0%; }
                .flex-shrink-0 { flex-shrink: 0; }
                .border { border-width: 1px; }
                .border-2 { border-width: 2px; }
                .border-gray-600 { border-color: #4b5563; }
                .border-gray-500 { border-color: #6b7280; }
                .border-gray-700 { border-color: #374151; }
                .border-blue-500 { border-color: #3b82f6; }
                .border-blue-600 { border-color: #2563eb; }
                .disabled\:opacity-50 { opacity: 0.5; }
                .cursor-pointer { cursor: pointer; }
                .cursor-not-allowed { cursor: not-allowed; }
                .z-50 { z-index: 50; }
                .z-40 { z-index: 40; }
                .select-none { user-select: none; }
                .opacity-80 { opacity: 0.8; }
                .transition { transition: all 0.2s ease; }
                .transition-all { transition: all 0.3s ease; }
                
                /* Custom scrollbar styling */
                ::-webkit-scrollbar { width: 8px; height: 8px; }
                ::-webkit-scrollbar-track { background: #1f2937; border-radius: 4px; }
                ::-webkit-scrollbar-thumb { background: #4b5563; border-radius: 4px; }
                ::-webkit-scrollbar-thumb:hover { background: #6b7280; }
                .scrollbar { scrollbar-width: thin; scrollbar-color: #4b5563 #1f2937; }
                
                /* WebDAV file list scrollbar */
                .webdav-file-list::-webkit-scrollbar { width: 6px; }
                .webdav-file-list::-webkit-scrollbar-track { background: #374151; }
                .webdav-file-list::-webkit-scrollbar-thumb { background: #6b7280; border-radius: 3px; }
                .webdav-file-list::-webkit-scrollbar-thumb:hover { background: #9ca3af; }
                .webdav-file-list { scrollbar-width: thin; scrollbar-color: #6b7280 #374151; }
                
                /* Modal overlay */
                .modal-overlay {
                    position: fixed;
                    top: 0;
                    left: 0;
                    right: 0;
                    bottom: 0;
                    background: rgba(0, 0, 0, 0.75);
                    display: flex: center;
                   ;
                    align-items justify-content: center;
                    z-index: 1000;
                    backdrop-filter: blur(4px);
                }
                
                .modal-content {
                    background: linear-gradient(180deg, #1f2937 0%, #111827 100%);
                    border-radius: 1rem;
                    padding: 2rem;
                    max-width: 90%;
                    max-height: 90%;
                    overflow: auto;
                    border: 1px solid #374151;
                    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
                }
                
                .modal-header {
                    font-size: 1.5rem;
                    font-weight: 700;
                    margin-bottom: 1.5rem;
                    color: #ffffff;
                    display: flex;
                    align-items: center;
                    gap: 0.75rem;
                }
                
                .modal-footer {
                    display: flex;
                    justify-content: flex-end;
                    gap: 1rem;
                    margin-top: 1.5rem;
                    padding-top: 1rem;
                    border-top: 1px solid #374151;
                }
                
                /* Input styling - FIXED for visibility */
                input[type="text"], 
                input[type="password"], 
                input[type="number"],
                input[type="url"],
                input[type="email"],
                textarea {
                    width: 100%;
                    padding: 0.75rem 1rem;
                    border-radius: 0.5rem;
                    background: #1f2937;
                    border: 2px solid #374151;
                    color: #f3f4f6;
                    font-size: 1rem;
                    transition: all 0.2s ease;
                }
                
                input[type="text"]::placeholder,
                input[type="password"]::placeholder,
                input[type="number"]::placeholder,
                input[type="url"]::placeholder,
                input[type="email"]::placeholder,
                textarea::placeholder {
                    color: #6b7280;
                }
                
                input[type="text"]:focus, 
                input[type="password"]:focus, 
                input[type="number"]:focus,
                input[type="url"]:focus,
                input[type="email"]:focus,
                textarea:focus {
                    outline: none;
                    border-color: #3b82f6;
                    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.3);
                    background: #1f2937;
                }
                
                input[type="text"]:hover, 
                input[type="password"]:hover, 
                input[type="number"]:hover,
                input[type="url"]:hover,
                input[type="email"]:hover,
                textarea:hover {
                    border-color: #4b5563;
                }
                
                /* Label styling */
                label {
                    display: block;
                    font-size: 0.875rem;
                    font-weight: 500;
                    color: #d1d5db;
                    margin-bottom: 0.5rem;
                }
                
                /* Input group */
                .input-group {
                    margin-bottom: 1.25rem;
                }
                
                .input-group:last-child {
                    margin-bottom: 0;
                }
                
                /* Range slider styling */
                input[type="range"] {
                    flex: 1;
                    height: 8px;
                    background: #374151;
                    border-radius: 4px;
                    appearance: none;
                    cursor: pointer;
                }
                
                input[type="range"]::-webkit-slider-thumb {
                    appearance: none;
                    width: 18px;
                    height: 18px;
                    background: linear-gradient(135deg, #3b82f6 0%, #8b5cf6 100%);
                    border-radius: 50%;
                    cursor: pointer;
                    box-shadow: 0 2px 6px rgba(59, 130, 246, 0.4);
                    transition: transform 0.2s ease;
                }
                
                input[type="range"]::-webkit-slider-thumb:hover {
                    transform: scale(1.1);
                }
                
                /* Checkbox styling */
                input[type="checkbox"] {
                    width: 1.125rem;
                    height: 1.125rem;
                    cursor: pointer;
                    accent-color: #3b82f6;
                }
                
                /* Button base styling */
                button {
                    cursor: pointer;
                    border: none;
                    transition: all 0.2s ease;
                    font-weight: 500;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    gap: 0.5rem;
                }
                
                button:disabled {
                    cursor: not-allowed;
                    opacity: 0.5;
                }
                
                /* Button variants */
                .btn {
                    padding: 0.75rem 1.5rem;
                    border-radius: 0.5rem;
                    font-size: 1rem;
                    font-weight: 500;
                }
                
                .btn-primary {
                    background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
                    color: white;
                    box-shadow: 0 4px 14px rgba(59, 130, 246, 0.4);
                }
                
                .btn-primary:hover:not(:disabled) {
                    background: linear-gradient(135deg, #2563eb 0%, #1d4ed8 100%);
                    transform: translateY(-1px);
                    box-shadow: 0 6px 20px rgba(59, 130, 246, 0.5);
                }
                
                .btn-secondary {
                    background: #374151;
                    color: #f3f4f6;
                    border: 1px solid #4b5563;
                }
                
                .btn-secondary:hover:not(:disabled) {
                    background: #4b5563;
                    border-color: #6b7280;
                }
                
                .btn-success {
                    background: linear-gradient(135deg, #22c55e 0%, #16a34a 100%);
                    color: white;
                    box-shadow: 0 4px 14px rgba(34, 197, 94, 0.4);
                }
                
                .btn-success:hover:not(:disabled) {
                    background: linear-gradient(135deg, #16a34a 0%, #15803d 100%);
                    transform: translateY(-1px);
                }
                
                .btn-danger {
                    background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
                    color: white;
                    box-shadow: 0 4px 14px rgba(239, 68, 68, 0.4);
                }
                
                .btn-danger:hover:not(:disabled) {
                    background: linear-gradient(135deg, #dc2626 0%, #b91c1c 100%);
                    transform: translateY(-1px);
                }
                
                .btn-sm {
                    padding: 0.5rem 1rem;
                    font-size: 0.875rem;
                }
                
                .btn-lg {
                    padding: 1rem 2rem;
                    font-size: 1.125rem;
                }
                
                .btn-icon {
                    padding: 0.75rem;
                    border-radius: 50%;
                }
                
                /* Progress bar */
                .progress-bar {
                    width: 100%;
                    height: 6px;
                    background: #374151;
                    border-radius: 3px;
                    overflow: hidden;
                }
                
                .progress-bar-fill {
                    height: 100%;
                    background: linear-gradient(90deg, #3b82f6 0%, #8b5cf6 100%);
                    border-radius: 3px;
                    transition: width 0.1s ease;
                }
                
                /* Card styling */
                .card {
                    background: linear-gradient(180deg, #1f2937 0%, #111827 100%);
                    border-radius: 0.75rem;
                    padding: 1.5rem;
                    border: 1px solid #374151;
                    box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.3);
                }
                
                .card-hover:hover {
                    border-color: #4b5563;
                    box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.4);
                }
                
                /* Track item styling */
                .track-item {
                    background: rgba(55, 65, 81, 0.5);
                    border: 1px solid transparent;
                    border-radius: 0.5rem;
                    padding: 0.75rem 1rem;
                    transition: all 0.2s ease;
                }
                
                .track-item:hover {
                    background: rgba(55, 65, 81, 0.8);
                    border-color: #4b5563;
                }
                
                .track-item.active {
                    background: rgba(59, 130, 246, 0.2);
                    border-color: #3b82f6;
                }
                
                /* Playlist styling */
                .playlist-item {
                    background: rgba(55, 65, 81, 0.5);
                    border: 2px solid transparent;
                    border-radius: 0.5rem;
                    padding: 0.75rem 1rem;
                    cursor: pointer;
                    transition: all 0.2s ease;
                }
                
                .playlist-item:hover {
                    background: rgba(55, 65, 81, 0.8);
                }
                
                .playlist-item.active {
                    background: rgba(59, 130, 246, 0.2);
                    border-color: #3b82f6;
                }
                
                /* Badge styling */
                .badge {
                    display: inline-flex;
                    align-items: center;
                    padding: 0.25rem 0.75rem;
                    font-size: 0.75rem;
                    font-weight: 500;
                    border-radius: 9999px;
                }
                
                .badge-blue {
                    background: rgba(59, 130, 246, 0.2);
                    color: #60a5fa;
                }
                
                .badge-green {
                    background: rgba(34, 197, 94, 0.2);
                    color: #4ade80;
                }
                
                .badge-red {
                    background: rgba(239, 68, 68, 0.2);
                    color: #f87171;
                }
                
                .badge-yellow {
                    background: rgba(234, 179, 8, 0.2);
                    color: #fbbf24;
                }
                
                /* Status indicator */
                .status-dot {
                    width: 8px;
                    height: 8px;
                    border-radius: 50%;
                    display: inline-block;
                }
                
                .status-dot.green {
                    background: #22c55e;
                    box-shadow: 0 0 8px rgba(34, 197, 94, 0.6);
                }
                
                .status-dot.yellow {
                    background: #eab308;
                    box-shadow: 0 0 8px rgba(234, 179, 8, 0.6);
                }
                
                .status-dot.red {
                    background: #ef4444;
                    box-shadow: 0 0 8px rgba(239, 68, 68, 0.6);
                }
                
                /* Animation keyframes */
                @keyframes pulse {
                    0%, 100% { opacity: 1; }
                    50% { opacity: 0.5; }
                }
                
                @keyframes spin {
                    from { transform: rotate(0deg); }
                    to { transform: rotate(360deg); }
                }
                
                @keyframes fadeIn {
                    from { opacity: 0; transform: translateY(10px); }
                    to { opacity: 1; transform: translateY(0); }
                }
                
                .animate-pulse {
                    animation: pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
                }
                
                .animate-spin {
                    animation: spin 1s linear infinite;
                }
                
                .animate-fade-in {
                    animation: fadeIn 0.3s ease-out;
                }
                
                /* Loading spinner */
                .spinner {
                    width: 24px;
                    height: 24px;
                    border: 3px solid #374151;
                    border-top-color: #3b82f6;
                    border-radius: 50%;
                    animation: spin 1s linear infinite;
                }
                
                /* Divider */
                .divider {
                    height: 1px;
                    background: linear-gradient(90deg, transparent, #374151, transparent);
                    margin: 1rem 0;
                }
                
                /* Icon styling */
                .icon {
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    width: 1.25rem;
                    height: 1.25rem;
                }
                
                /* Text utilities */
                .text-center { text-align: center; }
                .text-left { text-align: left; }
                .text-right { text-align: right; }
                
                /* Tooltip */
                .tooltip {
                    position: relative;
                }
                
                .tooltip::after {
                    content: attr(data-tooltip);
                    position: absolute;
                    bottom: 100%;
                    left: 50%;
                    transform: translateX(-50%);
                    padding: 0.5rem 0.75rem;
                    background: #111827;
                    color: #f3f4f6;
                    font-size: 0.75rem;
                    border-radius: 0.375rem;
                    white-space: nowrap;
                    opacity: 0;
                    visibility: hidden;
                    transition: all 0.2s ease;
                    z-index: 100;
                }
                
                .tooltip:hover::after {
                    opacity: 1;
                    visibility: visible;
                }
            </style>
        "#));

    dioxus_desktop::launch::launch_virtual_dom(VirtualDom::new(App), cfg);
}

#[component]
fn App() -> Element {
    let mut player_state = use_signal(|| PlayerState::Stopped);
    let mut current_track = use_signal(|| None::<TrackStub>);
    let mut current_time = use_signal(|| Duration::from_secs(0));
    let mut current_duration = use_signal(|| Duration::from_secs(0));
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

    // Provide current_time and duration as context for child components
    provide_context(current_time);
    provide_context(current_duration);

    // WebDAV Browser State
    let mut webdav_current_path = use_signal(|| "/".to_string());
    let mut webdav_items = use_signal(|| Vec::<webdav::WebDAVItem>::new());
    let mut webdav_is_loading = use_signal(|| false);
    let mut webdav_error = use_signal(|| Option::<String>::None);
    let mut current_lyric = use_signal(|| None::<player::Lyric>);
    let mut show_lyrics = use_signal(|| false);

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

                    // Sync duration from player
                    let duration = player.get_duration();
                    *current_duration.write() = duration;

                    // Sync lyrics from player
                    if let Some(lyric) = player.get_lyric() {
                        *current_lyric.write() = Some(lyric);
                    }

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
                                            player.play(path, Some(next_track.id.clone()));
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

    let header_icon = use_signal(|| load_header_icon());

    rsx! {
        div { class: "h-screen bg-gradient-to-b from-gray-900 to-black text-white overflow-y-auto flex flex-col",

            header { class: "bg-gray-800 shadow-lg p-6",
                div { class: "max-w-7xl mx-auto",
                    h1 { class: "text-4xl font-bold mb-2 flex items-center gap-3",
                        if let Some(icon_url) = header_icon.read().as_ref() {
                            img {
                                src: "{icon_url}",
                                alt: "Music Player Icon",
                                class: "w-8 h-8",
                            }
                        } else {
                            span { "🎵" }
                        }
                        "Dioxus Music Player"
                    }
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

            main { class: "flex-1 max-w-7xl mx-auto p-6 overflow-y-auto",

                div { class: "grid grid-cols-3 gap-6",

                    aside { class: "col-span-1 h-[calc(100vh-12rem)] overflow-y-auto",
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
                                                                        player
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
                                                                        player
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

                        PlayerControls {
                            state: player_state(),
                            duration: Some(current_duration()),
                            volume: volume(),
                            current_time,
                            on_play: move |_| {
                                if let Some(ref player) = *player_ref.read() {
                                    player.set_stopped_by_user(false);

                                    if player_state() == PlayerState::Paused && player.is_paused() {
                                        let _ = player.resume();
                                    } else if let Some(track_stub) = current_track() {
                                        player
                                            .play(
                                                std::path::Path::new(&track_stub.path),
                                                Some(track_stub.id.clone()),
                                            );
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
                                                    player.stop();
                                                    player.set_stopped_by_user(false);
                                                    player
                                                        .play(
                                                            std::path::Path::new(&prev_track.path),
                                                            Some(prev_track.id.clone()),
                                                        );
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
                                        if let Some(pos) = playlist
                                            .tracks
                                            .iter()
                                            .position(|t| t.id == current.id)
                                        {
                                            if pos < playlist.tracks.len() - 1 {
                                                let next_track = playlist.tracks[pos + 1].clone();
                                                if let Some(ref player) = *player_ref.read() {
                                                    player.stop();
                                                    player.set_stopped_by_user(false);
                                                    player
                                                        .play(
                                                            std::path::Path::new(&next_track.path),
                                                            Some(next_track.id.clone()),
                                                        );
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

                        NowPlayingCard {
                            current_track: current_track(),
                            player_ref: player_ref.clone(),
                        }

                        if let Some(lyric) = current_lyric() {
                            LyricsDisplay { current_time, lyric: Some(lyric) }
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

                        if playlists().len() > current_playlist() {
                            PlaylistTracks {
                                playlist: playlists()[current_playlist()].clone(),
                                current_track: current_track(),
                                on_track_select: move |track_stub: TrackStub| {
                                    if let Some(ref player) = *player_ref.read() {
                                        player.set_stopped_by_user(false);
                                        player
                                            .play(
                                                std::path::Path::new(&track_stub.path),
                                                Some(track_stub.id.clone()),
                                            );
                                        let _ = player.set_volume(volume());
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

                        // 保存到磁盘
                        let configs_to_save = configs.clone();
                        drop(configs);
                        if let Err(e) = save_webdav_configs(&configs_to_save) {
                            eprintln!("保存WebDAV配置失败: {}", e);
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
                                    encrypted_password: String::new(),
                                    enabled: false,
                                    password: None,
                                }
                            }
                        } else {
                            WebDAVConfig {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: String::new(),
                                url: String::new(),
                                username: String::new(),
                                encrypted_password: String::new(),
                                enabled: false,
                                password: None,
                            }
                        }
                    },
                    on_close: move |_| {
                        *show_webdav_config.write() = false;
                        *editing_webdav_config.write() = None;
                    },
                    on_save_config: move |new_config: WebDAVConfig| {
                        let editing_idx = editing_webdav_config();
                        let mut configs = webdav_configs.write();
                        if let Some(idx) = editing_idx {
                            if idx < configs.len() {
                                configs[idx] = new_config.clone();
                            }
                        } else {
                            configs.push(new_config);
                        }
                        let configs_to_save = configs.clone();
                        drop(configs);
                        if let Err(e) = save_webdav_configs(&configs_to_save) {
                            eprintln!("保存WebDAV配置失败: {}", e);
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

    // Track last fetched lyrics to avoid duplicates
    let mut last_lyric_track_info = use_signal(|| String::new());

    // Effect to fetch lyrics when metadata changes
    let player_ref_for_lyrics = player_ref.clone();
    use_effect(move || {
        let metadata = player_metadata();
        let player_option = player_ref_for_lyrics.read().clone();

        if let Some(ref p) = player_option {
            if let Some(m) = metadata.as_ref() {
                if let Some(title) = m.title.clone() {
                    if !title.is_empty() {
                        let artist = m.artist.clone().unwrap_or_default();
                        let track_info = format!("{}|{}", artist, title);
                        if *last_lyric_track_info.read() != track_info {
                            eprintln!("[Lyrics] 检测到新曲目: {} - {}", artist, title);

                            let player_for_task = p.clone();
                            let artist_for_search = artist.clone();
                            spawn(async move {
                                eprintln!("[Lyrics] 开始搜索歌词...");
                                player_for_task.fetch_lyrics_for_current_track(&title, &artist_for_search).await;
                                eprintln!("[Lyrics] 歌词搜索完成");
                            });

                            *last_lyric_track_info.write() = track_info;
                        }
                    }
                }
            }
        }
    });

    let _metadata_future = use_future(move || {
        let player_ref = player_ref.clone();
        let mut last_title = String::new();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                if let Some(ref player) = *player_ref.read() {
                    if let Some(metadata) = player.get_current_metadata() {
                        let title = metadata.title.clone().unwrap_or_default();
                        let artist = metadata.artist.clone().unwrap_or_default();
                        if title != last_title && !title.is_empty() {
                            eprintln!("[Metadata] 更新: {} - {}", artist, title);
                            last_title = title.clone();
                        }
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
fn LyricsDisplay(
    current_time: Signal<Duration>,
    lyric: Option<player::Lyric>,
) -> Element {
    let visible_lines = if let Some(ref lyric) = lyric {
        let current_idx = lyric.get_current_line(*current_time.read()).unwrap_or(0);
        let start = current_idx.saturating_sub(2);
        let end = (current_idx + 4).min(lyric.lines.len());
        lyric.lines[start..end].to_vec()
    } else {
        vec![]
    };

    rsx! {
        if !visible_lines.is_empty() {
            div { class: "bg-gray-800 rounded-lg p-6 mb-6 text-center",
                div { class: "space-y-3 max-h-48 overflow-y-auto",
                    for (idx , line) in visible_lines.iter().enumerate() {
                        div { class: "text-sm text-gray-400 transition-colors", "{line.text}" }
                    }
                }
            }
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
    let progress_percent = if let Some(d) = duration {
        if d.as_secs() > 0 {
            let ct = current_time();
            (ct.as_secs_f64() / d.as_secs_f64() * 100.0).clamp(0.0, 100.0) as i32
        } else {
            0
        }
    } else {
        0
    };

    let formatted_time = format_duration(current_time());
    let formatted_duration = duration.map(format_duration).unwrap_or_else(|| "0:00".to_string());

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 mb-6",

            div { class: "mb-4 relative",
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: "{progress_percent}",
                    class: "w-full h-2 appearance-none cursor-pointer bg-gray-700 rounded-full",
                    style: "accent-color: #3b82f6;",
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
                div { class: "border-t border-gray-700 pt-4",
                    h3 { class: "text-lg font-bold mb-2", "☁️ Cloud Sources" }
                    div { class: "max-h-96 overflow-y-auto space-y-2 webdav-file-list",
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
    let mut password = use_signal(|| config.get_password().unwrap_or_default());
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
                            let pwd = password();

                            let mut new_config = WebDAVConfig {
                                id: config.id.clone(),
                                name: name(),
                                url: url(),
                                username: username(),
                                encrypted_password: String::new(),
                                enabled: enabled(),
                                password: None,
                            };
                            if let Err(e) = new_config.set_password(&pwd) {
                                eprintln!("加密密码失败: {}", e);
                            }
                            on_save_config.call(new_config);
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
            } else if status.as_u16() == 429 {
                Err("请求过于频繁，请稍后再试 (HTTP 429)".to_string())
            } else if status.as_u16() == 404 {
                Err("服务器连接成功，但路径不存在 (HTTP 404)".to_string())
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

#[derive(Deserialize)]
struct OldWebDAVConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub enabled: bool,
}

#[derive(Serialize)]
struct ConfigForSave<'a> {
    id: &'a str,
    name: &'a str,
    url: &'a str,
    username: &'a str,
    encrypted_password: &'a str,
    enabled: bool,
}

// Load WebDAV configs from disk
fn load_webdav_configs() -> Result<Vec<WebDAVConfig>, Box<dyn std::error::Error>> {
    let config_dir = get_config_dir()?;
    let config_file = config_dir.join("webdav_configs.json");

    eprintln!("[Config] 配置文件路径: {}", config_file.display());

    if config_file.exists() {
        let content = std::fs::read_to_string(&config_file)?;
        
        // 尝试解析新格式
        let mut configs: Result<Vec<WebDAVConfig>, _> = serde_json::from_str(&content);
        
        // 如果新格式解析失败，尝试旧格式
        if configs.is_err() {
            let old_configs: Vec<OldWebDAVConfig> = serde_json::from_str(&content)?;
            let mut new_configs = Vec::new();
            
            for old in old_configs {
                let password_str = old.password.clone();
                let mut config = WebDAVConfig {
                    id: old.id,
                    name: old.name,
                    url: old.url,
                    username: old.username,
                    encrypted_password: String::new(),
                    enabled: old.enabled,
                    password: None,
                };
                let _ = config.set_password(&password_str);
                new_configs.push(config);
            }
            
            // 保存为新格式
            save_webdav_configs(&new_configs)?;
            return Ok(new_configs);
        }
        
        let mut configs = configs?;
        
        // 检查并迁移旧格式密码
        let mut needs_save = false;
        let mut passwords_to_migrate: Vec<(usize, String)> = Vec::new();
        
        for (i, config) in configs.iter().enumerate() {
            if let Some(ref pwd) = config.password {
                if !pwd.is_empty() {
                    needs_save = true;
                    passwords_to_migrate.push((i, pwd.clone()));
                }
            }
        }
        
        for (i, pwd) in passwords_to_migrate {
            if let Err(e) = configs[i].set_password(&pwd) {
                eprintln!("迁移密码失败: {}", e);
            }
            configs[i].password = None;
        }
        
        if needs_save {
            save_webdav_configs(&configs)?;
        }
        
        Ok(configs)
    } else {
        Ok(Vec::new())
    }
}

// Save WebDAV configs to disk
fn save_webdav_configs(configs: &[WebDAVConfig]) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir()?;

    let config_file = config_dir.join("webdav_configs.json");
    eprintln!("[Config] 保存配置文件到: {}", config_file.display());

    let json = serde_json::to_string_pretty(configs)?;
    std::fs::write(config_file, json)?;

    Ok(())
}

// Get config directory
fn get_config_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    // Cross-platform config directory
    if let Some(appdata) = std::env::var_os("APPDATA") {
        // Windows: %APPDATA%
        let path = std::path::PathBuf::from(appdata).join("dioxus_music");
        std::fs::create_dir_all(&path)?;
        eprintln!("[Config] 使用 Windows APPDATA 目录: {}", path.display());
        return Ok(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        // macOS/Linux: ~/.dioxus_music
        let path = std::path::PathBuf::from(home).join(".dioxus_music");
        std::fs::create_dir_all(&path)?;
        eprintln!("[Config] 使用 HOME 目录: {}", path.display());
        return Ok(path);
    }

    // Fallback: use current directory
    let path = std::path::PathBuf::from(".");
    std::fs::create_dir_all(&path)?;
    eprintln!("[Config] 使用当前目录作为配置目录: {}", path.display());
    Ok(path)
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
    
    let password = match config.get_password() {
        Ok(p) => {
            if p.len() == config.encrypted_password.len() {
                eprintln!("[WebDAV] 解密可能失败: 返回长度与密文相同");
            }
            eprintln!("[WebDAV] 解密结果: username={}, password_len={}", config.username, p.len());
            p
        }
        Err(e) => {
            eprintln!("[WebDAV] 解密失败: {}", e);
            String::new()
        }
    };
    
    eprintln!("[WebDAV] 准备请求: url={}{}, user={}", config.url, path, config.username);
    
    let client = WebDAVClient::new(config.url.clone())
        .with_auth(config.username.clone(), password);
    
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
        div { class: "bg-gray-800 rounded-lg p-4 h-full flex flex-col overflow-hidden",
            div { class: "flex justify-between items-center mb-4 flex-shrink-0",
                h3 { class: "text-lg font-bold truncate", "☁️ {config.name}" }
                button {
                    class: "text-gray-400 hover:text-white",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            // Path breadcrumb/navigation
            div { class: "flex gap-2 mb-2 text-sm flex-shrink-0",
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
                div { class: "bg-red-900 text-red-200 p-2 rounded mb-2 text-xs flex-shrink-0",
                    "{err}"
                }
            }

            div { class: "webdav-file-list flex-1 overflow-y-auto space-y-1 min-h-0",
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
    
    let password = config.get_password()?;
    
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).map_err(|_| "Invalid username")?;
        if !password.is_empty() {
            base_url.set_password(Some(&password)).map_err(|_| "Invalid password")?;
        }
    }
    
    for path_str in file_paths {
        let full_url = if path_str.starts_with("http") {
            path_str.to_string()
        } else {
            // 使用 reqwest::Url 正确构建 URL
            let mut url = base_url.clone();

            // 清理 path_str：移除开头的多余 / 和 , 符号
            let clean_path = path_str.trim_start_matches('/').trim_end_matches(',');

            // 将路径片段添加到 URL
            for segment in clean_path.split('/') {
                if !segment.is_empty() {
                    url = url.join(&format!("{}/", segment)).map_err(|_| "Invalid path segment")?;
                }
            }

            // 移除末尾的 /
            let mut url_str = url.to_string();
            if url_str.ends_with('/') && !clean_path.is_empty() {
                url_str.pop();
            }

            url_str
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
    
    let password = config.get_password()?;
    
    let client = reqwest::Client::new();
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).map_err(|_| "Invalid username")?;
        if !password.is_empty() {
            base_url.set_password(Some(&password)).map_err(|_| "Invalid password")?;
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
        
        let mut duration = std::time::Duration::from_secs(0);
        
        let temp_dir = std::env::temp_dir();
        let temp_filename = format!("dioxusmusic_{}", uuid::Uuid::new_v4());
        let temp_path = temp_dir.join(&temp_filename);
        
        match client.get(&full_url)
            .basic_auth(&config.username, Some(&password))
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
    
    let password = config.get_password()?;
    
    let client = reqwest::Client::new();
    let mut base_url = reqwest::Url::parse(&config.url)?;
    
    if !config.username.is_empty() {
        base_url.set_username(&config.username).ok();
        if !password.is_empty() {
            base_url.set_password(Some(&password)).ok();
        }
    }
    
    let full_url = if path.starts_with("http") {
        let mut u = reqwest::Url::parse(path)?;
        if !base_url.username().is_empty() {
            u.set_username(base_url.username()).ok();
            u.set_password(base_url.password()).ok();
        }
        u.to_string()
    } else {
        base_url.join(path)?.to_string()
    };
    
    let temp_dir = std::env::temp_dir();
    let temp_filename = format!("dioxusmusic_{}", uuid::Uuid::new_v4());
    let temp_path = temp_dir.join(&temp_filename);
    
    match client.get(&full_url)
        .basic_auth(&config.username, Some(&password))
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

