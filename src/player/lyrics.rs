use base64::{Engine, prelude::BASE64_STANDARD};
use reqwest::Client;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn decode_html_entities(text: &str) -> String {
    let mut result = text.to_string();
    let replacements = [
        ("&amp;", "&"),
        ("&apos;", "'"),
        ("&quot;", "\""),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&nbsp;", " "),
        ("&#39;", "'"),
        ("&#x27;", "'"),
        ("&#34;", "\""),
        ("&#60;", "<"),
        ("&#62;", ">"),
        ("&copy;", "©"),
        ("&reg;", "®"),
        ("&trade;", "™"),
    ];

    for (entity, replacement) in &replacements {
        result = result.replace(entity, replacement);
    }

    result
}

#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub time: Duration,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lyric {
    pub title: String,
    pub artist: String,
    pub lines: Vec<LyricLine>,
}

impl Lyric {
    pub fn empty() -> Self {
        Lyric {
            title: String::new(),
            artist: String::new(),
            lines: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn get_current_line(&self, current_time: Duration) -> Option<usize> {
        for (i, line) in self.lines.iter().enumerate() {
            if line.time > current_time {
                if i == 0 {
                    return Some(0);
                }
                return Some(i - 1);
            }
        }
        if self.lines.is_empty() {
            return None;
        }
        Some(self.lines.len().saturating_sub(1))
    }
}

pub async fn search_lyrics(
    title: &str,
    artist: &str,
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    let client = Client::new();

    let query = format!("{} {}", artist, title);

    let response = match client
        .get("https://music.163.com/api/search/get/")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .header("Referer", "https://music.163.com/")
        .query(&[("s", query.as_str()), ("type", "1"), ("limit", "1"), ("offset", "0")])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-Search] 请求失败: {}", e);
                return Ok(None);
            }
        };

    if !response.status().is_success() {
        return Ok(None);
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let search_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let empty_vec: Vec<serde_json::Value> = Vec::new();
    let songs = search_result["result"]["songs"]
        .as_array()
        .unwrap_or(&empty_vec);

    if let Some(first_song) = songs.first() {
        let song_id = match first_song["id"].as_u64() {
            Some(id) => id.to_string(),
            None => return Ok(None),
        };
        let song_name = first_song["name"].as_str().unwrap_or("").to_string();
        let artist_name = first_song["artists"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|a| a["name"].as_str())
            .unwrap_or("")
            .to_string();

        return Ok(Some((song_id, format!("{} - {}", artist_name, song_name))));
    }

    Ok(None)
}

pub async fn search_all_lyrics(
    title: &str,
    artist: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let client = Client::new();

    let query = format!("{} {}", artist, title);

    let response = match client
        .get("https://music.163.com/api/search/get/")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .header("Referer", "https://music.163.com/")
        .query(&[("s", query.as_str()), ("type", "1"), ("limit", "10"), ("offset", "0")])
        .send()
        .await {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()),
        };

    if !response.status().is_success() {
        return Ok(Vec::new());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let search_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    let empty_vec: Vec<serde_json::Value> = Vec::new();
    let songs = search_result["result"]["songs"]
        .as_array()
        .unwrap_or(&empty_vec);

    let mut results = Vec::new();
    for song in songs.iter().take(10) {
        if let (Some(id), Some(name)) = (song["id"].as_u64(), song["name"].as_str()) {
            let artist_name = song["artists"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|a| a["name"].as_str())
                .unwrap_or("")
                .to_string();
            results.push((id.to_string(), format!("{} - {}", artist_name, name)));
        }
    }

    Ok(results)
}

pub async fn download_lyrics(
    song_id: &str,
) -> Result<Lyric, Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = match client
        .get("https://music.163.com/api/song/lyric")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .header("Referer", "https://music.163.com/")
        .query(&[("id", song_id), ("lv", "1")])
        .send()
        .await {
            Ok(r) => r,
            Err(_) => return Ok(Lyric::empty()),
        };

    if !response.status().is_success() {
        return Ok(Lyric::empty());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(_) => return Ok(Lyric::empty()),
    };

    let lyric_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(Lyric::empty()),
    };

    let title = lyric_result["songName"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let artist = lyric_result["artist"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let lrc_content = lyric_result["lrc"]
        .as_str()
        .unwrap_or("");

    if lrc_content.is_empty() {
        return Ok(Lyric::empty());
    }

    let lines = parse_lrc(lrc_content);

    Ok(Lyric {
        title,
        artist,
        lines,
    })
}

fn parse_lrc(content: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((time_str, text)) = line.split_once(']') {
            if let Some(time_str) = time_str.strip_prefix('[') {
                if let Some(duration) = parse_time(time_str) {
                    lines.push(LyricLine {
                        time: duration,
                        text: text.trim().to_string(),
                    });
                }
            }
        }
    }

    lines.sort_by_key(|l| l.time);
    lines
}

fn parse_time(time_str: &str) -> Option<Duration> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: u64 = match parts[0].parse() {
        Ok(m) => m,
        Err(_) => return None,
    };

    let seconds_parts: Vec<&str> = parts[1].split('.').collect();
    let seconds: u64 = match seconds_parts[0].parse() {
        Ok(s) => s,
        Err(_) => return None,
    };

    let millis: u64 = if seconds_parts.len() > 1 {
        let ms_str = &seconds_parts[1][0..std::cmp::min(seconds_parts[1].len(), 2)];
        ms_str.parse().unwrap_or(0) * 10
    } else {
        0
    };

    Some(Duration::from_secs(minutes * 60 + seconds) + Duration::from_millis(millis))
}

pub async fn search_kugou_lyrics(
    title: &str,
    artist: &str,
) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
    let client = Client::new();

    let query = format!("{} {}", artist, title);

    let response = match client
        .get("http://mobilecdn.kugou.com/api/v3/search/song")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .query(&[
            ("keyword", query.as_str()),
            ("page", "1"),
            ("pagesize", "10"),
        ])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-Kugou] 搜索请求失败: {}", e);
                return Ok(Vec::new());
            }
        };

    if !response.status().is_success() {
        eprintln!("[Lyrics-Kugou] 搜索 HTTP 错误: {}", response.status());
        return Ok(Vec::new());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] 读取响应失败: {}", e);
            return Ok(Vec::new());
        }
    };

    let search_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] JSON 解析失败: {}", e);
            return Ok(Vec::new());
        }
    };

    let empty_vec: Vec<serde_json::Value> = Vec::new();
    let songs = search_result["data"]["info"]
        .as_array()
        .unwrap_or(&empty_vec);

    eprintln!("[Lyrics-Kugou] 找到 {} 首歌曲", songs.len());

    let mut results = Vec::new();
    for song in songs.iter().take(10) {
        if let (Some(hash), Some(album_id), Some(songname)) = (
            song["hash"].as_str(),
            song["album_id"].as_str(),
            song["songname_original"].as_str()
        ) {
            let singer = song["singername"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let album = song["album_name"]
                .as_str()
                .unwrap_or("")
                .to_string();
            results.push((hash.to_string(), album_id.to_string(), format!("{} - {}", singer, songname)));
        }
    }

    Ok(results)
}

pub async fn download_kugou_lyric(
    hash: &str,
    album_id: &str,
) -> Result<Lyric, Box<dyn std::error::Error>> {
    let client = Client::new();

    let search_response = match client
        .get("http://krcs.kugou.com/search")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .query(&[
            ("hash", hash),
            ("album_id", album_id),
            ("ver", "1"),
            ("client", "pc"),
            ("man", "yes"),
        ])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-Kugou] 搜索歌词失败: {}", e);
                return Ok(Lyric::empty());
            }
        };

    if !search_response.status().is_success() {
        eprintln!("[Lyrics-Kugou] 搜索歌词 HTTP 错误: {}", search_response.status());
        return Ok(Lyric::empty());
    }

    let text = match search_response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] 读取搜索响应失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let search_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] 搜索响应 JSON 解析失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let candidates: Vec<serde_json::Value> = match search_result["candidates"].as_array() {
        Some(arr) => arr.clone(),
        None => {
            eprintln!("[Lyrics-Kugou] 未找到候选歌词");
            return Ok(Lyric::empty());
        }
    };

    if candidates.is_empty() {
        eprintln!("[Lyrics-Kugou] 未找到候选歌词");
        return Ok(Lyric::empty());
    }

    let first_candidate = &candidates[0];

    let accesskey = match first_candidate["accesskey"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("[Lyrics-Kugou] accesskey 为空");
            return Ok(Lyric::empty());
        }
    };

    let download_id = match first_candidate["download_id"].as_str() {
        Some(s) => s.to_string(),
        None => String::from("1"),
    };

    let singer = first_candidate["singer"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let song_name = first_candidate["song"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let download_response = match client
        .get("http://lyrics.kugou.com/download")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .query(&[
            ("accesskey", accesskey.as_str()),
            ("id", download_id.as_str()),
            ("ver", "1"),
            ("client", "pc"),
            ("fmt", "lrc"),
            ("charset", "utf8"),
        ])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-Kugou] 下载歌词失败: {}", e);
                return Ok(Lyric::empty());
            }
        };

    if !download_response.status().is_success() {
        eprintln!("[Lyrics-Kugou] 下载 HTTP 错误: {}", download_response.status());
        return Ok(Lyric::empty());
    };

    let download_text = match download_response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] 读取下载响应失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let download_result: serde_json::Value = match serde_json::from_str(&download_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] 下载响应 JSON 解析失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let content = match download_result["content"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("[Lyrics-Kugou] 歌词内容为空");
            return Ok(Lyric::empty());
        }
    };

    if content.is_empty() {
        eprintln!("[Lyrics-Kugou] 歌词内容为空");
        return Ok(Lyric::empty());
    }

    let decoded = match BASE64_STANDARD.decode(&content) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] Base64 解码失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let lrc_content = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Lyrics-Kugou] UTF8 解码失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    if lrc_content.is_empty() {
        eprintln!("[Lyrics-Kugou] 解码后歌词为空");
        return Ok(Lyric::empty());
    }

    let lrc_content = decode_html_entities(&lrc_content);
    let lines = parse_lrc(&lrc_content);

    eprintln!("[Lyrics-Kugou] 解析到 {} 行歌词", lines.len());

    Ok(Lyric {
        title: song_name,
        artist: singer,
        lines,
    })
}

pub async fn search_qqmusic_lyrics(
    title: &str,
    artist: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let client = Client::new();

    let query = format!("{} {}", artist, title);

    let response = match client
        .get("https://c.y.qq.com/soso/fcgi-bin/client_search_cp")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/102.0.5005.63 Safari/537.36")
        .header("Referer", "https://y.qq.com/n/ryqq/player")
        .header("Host", "c.y.qq.com")
        .header("Origin", "https://y.qq.com")
        .query(&[
            ("w", query.as_str()),
            ("format", "json"),
            ("p", "1"),
            ("n", "10"),
            ("cr", "1"),
            ("t", "0"),
        ])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-QQ] 搜索请求失败: {}", e);
                return Ok(Vec::new());
            }
        };

    if !response.status().is_success() {
        eprintln!("[Lyrics-QQ] 搜索 HTTP 错误: {}", response.status());
        return Ok(Vec::new());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-QQ] 读取响应失败: {}", e);
            return Ok(Vec::new());
        }
    };

    let search_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-QQ] JSON 解析失败: {}", e);
            return Ok(Vec::new());
        }
    };

    let empty_vec: Vec<serde_json::Value> = Vec::new();
    let songs = search_result["data"]["song"]["list"]
        .as_array()
        .unwrap_or(&empty_vec);

    eprintln!("[Lyrics-QQ] 找到 {} 首歌曲", songs.len());

    let mut results = Vec::new();
    for song in songs.iter().take(10) {
        if let (Some(songmid), Some(songname)) = (
            song["songmid"].as_str(),
            song["songname"].as_str()
        ) {
            let singer = song["singer"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|s| s["name"].as_str())
                .unwrap_or("")
                .to_string();
            results.push((songmid.to_string(), format!("{} - {}", singer, songname)));
        }
    }

    Ok(results)
}

pub async fn download_qqmusic_lyric(
    songmid: &str,
) -> Result<Lyric, Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = match client
        .get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/102.0.5005.63 Safari/537.36")
        .header("Referer", "https://y.qq.com/n/ryqq/player")
        .header("Host", "c.y.qq.com")
        .header("Origin", "https://y.qq.com")
        .query(&[
            ("songmid", songmid),
            ("format", "json"),
            ("g_tk", "5381"),
        ])
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-QQ] 下载请求失败: {}", e);
                return Ok(Lyric::empty());
            }
        };

    if !response.status().is_success() {
        eprintln!("[Lyrics-QQ] 下载 HTTP 错误: {}", response.status());
        return Ok(Lyric::empty());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-QQ] 读取响应失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let lyric_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-QQ] JSON 解析失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let lyric_content = match lyric_result["lyric"].as_str() {
        Some(s) => s.to_string(),
        None => {
            eprintln!("[Lyrics-QQ] 歌词字段为空");
            return Ok(Lyric::empty());
        }
    };

    if lyric_content.is_empty() {
        eprintln!("[Lyrics-QQ] 歌词内容为空");
        return Ok(Lyric::empty());
    }

    let decoded = match BASE64_STANDARD.decode(&lyric_content) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[Lyrics-QQ] Base64 解码失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let lrc_content = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Lyrics-QQ] UTF8 解码失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    if lrc_content.is_empty() {
        eprintln!("[Lyrics-QQ] 解码后歌词为空");
        return Ok(Lyric::empty());
    }

    let lrc_content = decode_html_entities(&lrc_content);

    let title = lyric_result["songName"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let artist = lyric_result["singer"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let lines = parse_lrc(&lrc_content);

    eprintln!("[Lyrics-QQ] 解析到 {} 行歌词", lines.len());

    Ok(Lyric {
        title,
        artist,
        lines,
    })
}

pub async fn fetch_lyrics_for_track(
    title: &str,
    artist: &str,
    embedded_lyrics: Option<&str>,
    music_path: Option<&Path>,
) -> Result<Lyric, Box<dyn std::error::Error>> {
    if title.is_empty() {
        return Ok(Lyric::empty());
    }

    let artist_for_search = if artist.is_empty() { "" } else { artist };

    eprintln!("[Lyrics] 搜索歌词: {} - {}", artist_for_search, title);

    // 1. 优先使用内嵌歌词
    if let Some(embedded) = embedded_lyrics {
        if !embedded.is_empty() {
            eprintln!("[Lyrics] 找到内嵌歌词");
            let embedded = decode_html_entities(embedded);
            let lines = parse_lrc(&embedded);
            if !lines.is_empty() {
                return Ok(Lyric {
                    title: title.to_string(),
                    artist: artist.to_string(),
                    lines,
                });
            }
        }
    }

    // 2. 尝试加载本地歌词文件
    if let Some(path) = music_path {
        if let Some(lyric_path) = find_local_lyric(path) {
            eprintln!("[Lyrics] 找到本地歌词文件: {:?}", lyric_path);
            match load_local_lyric(&lyric_path) {
                Ok(lyric) if !lyric.is_empty() => {
                    eprintln!("[Lyrics] 本地歌词加载成功");
                    return Ok(lyric);
                }
                _ => {
                    eprintln!("[Lyrics] 本地歌词解析失败");
                }
            }
        } else {
            eprintln!("[Lyrics] 未找到本地歌词文件");
        }
    }

    // 3. 尝试QQ音乐
    match search_qqmusic_lyrics(title, artist_for_search).await {
        Ok(qq_songs) if !qq_songs.is_empty() => {
            eprintln!("[Lyrics] QQ音乐找到 {} 首候选歌曲", qq_songs.len());

            for (songmid, song_name) in qq_songs {
                eprintln!("[Lyrics] 尝试QQ: {}", song_name);
                match download_qqmusic_lyric(&songmid).await {
                    Ok(lyric) if !lyric.is_empty() => {
                        eprintln!("[Lyrics] QQ音乐歌词获取成功");
                        return Ok(lyric);
                    }
                    _ => {
                        eprintln!("[Lyrics] QQ版本 {} 无歌词，继续尝试...", songmid);
                    }
                }
            }
            eprintln!("[Lyrics] QQ音乐所有版本均无歌词");
        }
        Ok(_) => {
            eprintln!("[Lyrics] QQ音乐未找到歌曲");
        }
        Err(e) => {
            eprintln!("[Lyrics] QQ音乐搜索失败: {}", e);
        }
    }

    // 4. 尝试酷狗音乐
    match search_kugou_lyrics(title, artist_for_search).await {
        Ok(kugou_songs) if !kugou_songs.is_empty() => {
            eprintln!("[Lyrics] 酷狗找到 {} 首候选歌曲", kugou_songs.len());

            for (hash, album_id, song_name) in kugou_songs {
                eprintln!("[Lyrics] 尝试酷狗: {}", song_name);
                match download_kugou_lyric(&hash, &album_id).await {
                    Ok(lyric) if !lyric.is_empty() => {
                        eprintln!("[Lyrics] 酷狗歌词获取成功");
                        return Ok(lyric);
                    }
                    _ => {
                        eprintln!("[Lyrics-酷狗] 版本 {} 无歌词，继续尝试...", hash);
                    }
                }
            }
            eprintln!("[Lyrics] 酷狗所有版本均无歌词");
        }
        Ok(_) => {
            eprintln!("[Lyrics] 酷狗未找到歌曲");
        }
        Err(e) => {
            eprintln!("[Lyrics] 酷狗搜索失败: {}", e);
        }
    }

    // 5. 尝试 OVH API
    eprintln!("[Lyrics] 尝试 OVH API...");
    match download_ovh_lyric(artist_for_search, title).await {
        Ok(lyric) if !lyric.is_empty() => {
            eprintln!("[Lyrics] OVH 歌词获取成功");
            return Ok(lyric);
        }
        _ => {
            eprintln!("[Lyrics] OVH 未找到歌词");
        }
    }

    eprintln!("[Lyrics] 所有来源均无歌词");
    Ok(Lyric::empty())
}

pub async fn download_ovh_lyric(
    artist: &str,
    title: &str,
) -> Result<Lyric, Box<dyn std::error::Error>> {
    let client = Client::new();

    let encoded_artist = urlencoding::encode(artist);
    let encoded_title = urlencoding::encode(title);

    let api_url = format!("https://api.lyrics.ovh/v1/{}/{}", encoded_artist, encoded_title);

    let response = match client
        .get(&api_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/102.0.5005.63 Safari/537.36")
        .header("Accept", "application/json")
        .send()
        .await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Lyrics-OVH] 请求失败: {}", e);
                return Ok(Lyric::empty());
            }
        };

    if !response.status().is_success() {
        eprintln!("[Lyrics-OVH] HTTP 错误: {}", response.status());
        return Ok(Lyric::empty());
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Lyrics-OVH] 读取响应失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let json_result: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[Lyrics-OVH] JSON 解析失败: {}", e);
            return Ok(Lyric::empty());
        }
    };

    let lyrics = match json_result["lyrics"].as_str() {
        Some(s) => s,
        None => {
            eprintln!("[Lyrics-OVH] 歌词字段为空");
            return Ok(Lyric::empty());
        }
    };

    if lyrics.is_empty() {
        eprintln!("[Lyrics-OVH] 歌词内容为空");
        return Ok(Lyric::empty());
    }

    let lyrics = decode_html_entities(lyrics);
    let lines = parse_lrc(&lyrics);

    eprintln!("[Lyrics-OVH] 解析到 {} 行歌词", lines.len());

    Ok(Lyric {
        title: title.to_string(),
        artist: artist.to_string(),
        lines,
    })
}

pub fn load_local_lyric(file_path: &Path) -> Result<Lyric, Box<dyn std::error::Error>> {
    match fs::read_to_string(file_path) {
        Ok(content) => {
            let content = decode_html_entities(&content);
            let lines = parse_lrc(&content);
            Ok(Lyric {
                title: String::new(),
                artist: String::new(),
                lines,
            })
        }
        Err(_) => Ok(Lyric::empty()),
    }
}

pub fn find_local_lyric(music_path: &Path) -> Option<std::path::PathBuf> {
    let base_name = music_path.file_stem()?.to_string_lossy();

    for ext in &["lrc", "txt"] {
        let lyric_path = music_path.with_file_name(format!("{}.{}", base_name, ext));
        if lyric_path.exists() {
            return Some(lyric_path);
        }
    }

    for sibling in music_path.parent()?.read_dir().ok()?.flatten() {
        let path = sibling.path();
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy()) {
            if name.to_lowercase().contains(&base_name.to_lowercase())
                && (name.ends_with(".lrc") || name.ends_with(".txt")) {
                return Some(path);
            }
        }
    }

    None
}
