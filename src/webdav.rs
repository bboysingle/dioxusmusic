use reqwest::Client;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct WebDAVClient {
    client: Arc<Client>,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WebDAVItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

impl WebDAVClient {
    pub fn new(base_url: String) -> Self {
        // 确保 base_url 末尾没有多余的 /
        let clean_url = base_url.trim_end_matches('/').to_string();
        WebDAVClient {
            client: Arc::new(Client::new()),
            base_url: clean_url,
            username: None,
            password: None,
        }
    }

    pub fn with_auth(mut self, username: String, password: String) -> Self {
        self.username = Some(username);
        self.password = Some(password);
        self
    }

    pub async fn list_files(&self, path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.base_url, path);
        
        // Use a generic request for PROPFIND since reqwest doesn't have propfind method
        let mut req = self.client.request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url);
        
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user.clone(), Some(pass.clone()));
        }

        let response = req.send().await?;
        
        // Parse WebDAV response (simplified - would need proper XML parsing)
        let text: String = response.text().await?;
        let files = parse_webdav_response(&text);
        
        Ok(files)
    }

    pub async fn list_items(&self, path: &str) -> Result<Vec<WebDAVItem>, Box<dyn std::error::Error>> {
        let normalized_path = if !path.starts_with('/') {
            format!("/{}", path)
        } else {
            path.to_string()
        };
        
        let url = format!("{}{}", self.base_url, normalized_path);
        
        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
    <D:getcontentlength/>
    <D:getlastmodified/>
  </D:prop>
</D:propfind>"#;
        
        let mut req = self.client.request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url);
        req = req.header("Depth", "1");
        req = req.header("Content-Type", "application/xml; charset=\"utf-8\"");
        req = req.body(propfind_body.to_string());
        
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user.clone(), Some(pass.clone()));
        }

        let response = req.send().await?;
        
        let status = response.status();
        let text: String = response.text().await?;
        
        // 调试：打印响应状态和内容（如果是开发环境）
        #[cfg(debug_assertions)]
        {
            eprintln!("[WebDAV] URL: {}", url);
            eprintln!("[WebDAV] Status: {}", status);
            eprintln!("[WebDAV] Response length: {} bytes", text.len());
            if !text.is_empty() {
                eprintln!("[WebDAV] Response preview (first 1000 chars):\n{}", &text[..std::cmp::min(1000, text.len())]);
            }
        }
        
        if !status.is_success() {
            return Err(format!("WebDAV 请求失败 (HTTP {}): {}", status, text).into());
        }
        
        if text.is_empty() {
            return Err("WebDAV 服务器返回空响应".into());
        }
        
        let items = parse_webdav_items(&text, &self.base_url);
        
        #[cfg(debug_assertions)]
        {
            eprintln!("[WebDAV] Parsed {} items", items.len());
        }
        
        Ok(items)
    }

    pub async fn download_file(
        &self,
        path: &str,
        dest: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.base_url, path);
        
        let mut req = self.client.get(&url);
        
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user.clone(), Some(pass.clone()));
        }

        let response = req.send().await?;
        let bytes = response.bytes().await?;
        
        tokio::fs::write(dest, bytes).await?;
        Ok(())
    }

    pub async fn upload_file(
        &self,
        src: &str,
        dest: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}{}", self.base_url, dest);
        let bytes = tokio::fs::read(src).await?;
        
        let mut req = self.client.put(&url)
            .body(bytes);
        
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user.clone(), Some(pass.clone()));
        }

        req.send().await?;
        Ok(())
    }
}

fn parse_webdav_response(response: &str) -> Vec<String> {
    // Simple parsing - in production use proper XML parser
    let mut files = Vec::new();
    
    for line in response.lines() {
        if line.contains("<D:href>") {
            let start = line.find("<D:href>").map(|i| i + 8).unwrap_or(0);
            let end = line.find("</D:href>").unwrap_or(line.len());
            let href = &line[start..end];
            
            if !href.is_empty() && !href.ends_with('/') {
                files.push(href.to_string());
            }
        }
    }
    
    files
}

fn parse_webdav_items(response: &str, base_url: &str) -> Vec<WebDAVItem> {
    let mut items = Vec::new();
    
    let responses = response
        .split("<D:response")
        .skip(1)
        .collect::<Vec<&str>>();
    
    for response_part in responses {
        if !response_part.contains("</D:response>") {
            continue;
        }
        
        let response_end = match response_part.find("</D:response>") {
            Some(pos) => pos,
            None => continue,
        };
        
        let clean_part = &response_part[..response_end];
        
        let mut href = String::new();
        let mut displayname = String::new();
        let mut is_collection = false;
        let mut size = 0u64;
        let mut modified = String::new();
        
        // 解析href - 查找第一个<D:href>...</D:href>
        if let Some(href_start) = clean_part.find("<D:href>") {
            let content_start = href_start + 8;
            if let Some(href_end) = clean_part[content_start..].find("</D:href>") {
                href = clean_part[content_start..content_start + href_end].to_string();
            }
        }
        
        // 解析displayname
        if let Some(name_start) = clean_part.find("<D:displayname>") {
            let content_start = name_start + 15;
            if let Some(name_end) = clean_part[content_start..].find("</D:displayname>") {
                displayname = clean_part[content_start..content_start + name_end].to_string();
            }
        }
        
        // 解析resourcetype - 检查是否是collection
        // 格式可能是 <D:collection/> 或 <D:collection xmlns:D="DAV:"/>
        if clean_part.contains("<D:collection") {
            is_collection = true;
        }
        
        // 解析getcontentlength - 需要在<D:propstat>中找到正确的值
        // 跳过404的部分（404表示该属性不存在）
        if let Some(prop_start) = clean_part.find("<D:propstat>") {
            let prop_section = &clean_part[prop_start..];
            if let Some(first_prop) = prop_section.find("<D:prop>") {
                let after_first_prop = &prop_section[first_prop + 8..];
                if let Some(first_prop_end) = after_first_prop.find("</D:prop>") {
                    let first_prop_content = &after_first_prop[..first_prop_end];
                    if !first_prop_content.contains("<D:getcontentlength>") 
                        || first_prop_content.contains("<D:collection") {
                        if let Some(size_start) = first_prop_content.find("<D:getcontentlength>") {
                            let size_content_start = size_start + 20;
                            if let Some(size_end) = first_prop_content[size_content_start..].find("</D:getcontentlength>") {
                                let size_str = &first_prop_content[size_content_start..size_content_start + size_end];
                                if !size_str.is_empty() {
                                    size = size_str.parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 解析getlastmodified
        if let Some(mod_start) = clean_part.find("<D:getlastmodified>") {
            let content_start = mod_start + 19;
            if let Some(mod_end) = clean_part[content_start..].find("</D:getlastmodified>") {
                modified = clean_part[content_start..content_start + mod_end].to_string();
            }
        }
        
        // 提取名称
        let name = if !displayname.is_empty() {
            displayname
        } else {
            extract_name_from_path(&href)
        };
        
        // 过滤根目录
        if !href.is_empty() && href != "/" && !name.is_empty() {
            // base_url 是配置的 URL（如 http://x:5244/dav/tianyi）
            // href 是服务器返回的完整路径（如 /dav/tianyi/音乐/xxx.mp3）
            // item_path 应该是相对于当前浏览目录的路径（不包括配置的子目录前缀）
            let item_path = if href.starts_with("/dav/") {
                // 去掉 /dav/ 前缀
                let after_dav = &href[5..];

                // 提取 base_url 中的子目录部分（如 tianyi）
                let base_sub_path = if let Some(pos) = base_url.find("/dav/") {
                    let after_base_dav = &base_url[pos + 5..]; // 去掉 /dav
                    after_base_dav.trim_start_matches('/').to_string()
                } else {
                    String::new()
                };

                // 如果 after_dav 以 base_sub_path 开头，去掉它
                if !base_sub_path.is_empty() && after_dav.starts_with(&base_sub_path) {
                    let after_base = &after_dav[base_sub_path.len()..];
                    after_base.trim_start_matches('/').to_string()
                } else {
                    after_dav.to_string()
                }
            } else if href.starts_with(base_url) {
                href[base_url.len()..].to_string()
            } else {
                href.trim_start_matches('/').to_string()
            };

            eprintln!("[WebDAV] item_path='{}'", item_path);

            items.push(WebDAVItem {
                name,
                path: item_path,
                is_dir: is_collection,
                size,
                modified,
            });
        }
    }

    items
}

fn extract_xml_content(xml: &str, tag: &str) -> Option<String> {
    let tag_upper = tag.to_uppercase();
    let tag_lower = tag.to_lowercase();
    
    let search_patterns = vec![
        format!("<D:{}>", tag_upper),
        format!("<d:{}>", tag_lower),
        format!("<D:{}>", tag_lower),
        format!("<{}>", tag),
    ];
    
    for pattern in search_patterns {
        let start_tag = &pattern;
        let end_tag = format!("</{}>", tag_upper);
        
        if let Some(start_idx) = xml.find(start_tag) {
            let content_start = start_idx + start_tag.len();
            if let Some(end_idx) = xml[content_start..].find(&end_tag) {
                let full_end = content_start + end_idx;
                return Some(xml[content_start..full_end].to_string());
            }
        }
    }
    
    None
}

fn extract_name_from_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return String::new();
    }
    
    let clean_path = path.trim_end_matches('/');
    
    if let Ok(decoded) = urlencoding::decode(clean_path) {
        let decoded_str = decoded.to_string();
        if let Some(pos) = decoded_str.rfind('/') {
            return decoded_str[pos + 1..].to_string();
        }
        return decoded_str;
    }
    
    if let Some(pos) = clean_path.rfind('/') {
        return clean_path[pos + 1..].to_string();
    }
    
    clean_path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_webdav_response() {
        let response = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/music/song1.mp3</D:href>
  </D:response>
</D:multistatus>"#;
        
        let files = parse_webdav_response(response);
        assert!(files.contains(&"/music/song1.mp3".to_string()));
    }
}
