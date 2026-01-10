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
        WebDAVClient {
            client: Arc::new(Client::new()),
            base_url,
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
        let url = format!("{}{}", self.base_url, path);
        
        let mut req = self.client.request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url);
        req = req.header("Depth", "1");
        
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req = req.basic_auth(user.clone(), Some(pass.clone()));
        }

        let response = req.send().await?;
        let text: String = response.text().await?;
        
        let items = parse_webdav_items(&text, &self.base_url);
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
    let lines: Vec<&str> = response.lines().collect();
    
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        
        if line.contains("<D:response>") {
            let mut href = String::new();
            let mut is_collection = false;
            let mut size = 0u64;
            let mut modified = String::new();
            
            // Parse within this response block until </D:response>
            i += 1;
            while i < lines.len() && !lines[i].contains("</D:response>") {
                let curr = lines[i];
                
                if curr.contains("<D:href>") {
                    if let (Some(start), Some(end)) = (curr.find(">"), curr.find("</")) {
                        href = curr[start + 1..end].to_string();
                    }
                }
                
                if curr.contains("resourcetype") && curr.contains("collection") {
                    is_collection = true;
                }
                
                if curr.contains("<D:getcontentlength>") {
                    if let (Some(start), Some(end)) = (curr.find(">"), curr.find("</")) {
                        let size_str = &curr[start + 1..end];
                        size = size_str.parse().unwrap_or(0);
                    }
                }
                
                if curr.contains("<D:getlastmodified>") {
                    if let (Some(start), Some(end)) = (curr.find(">"), curr.find("</")) {
                        modified = curr[start + 1..end].to_string();
                    }
                }
                
                i += 1;
            }
            
            // Skip the base URL path itself
            if !href.is_empty() && href != "/" && !href.ends_with(base_url) {
                let name = if href.ends_with('/') {
                    href.trim_end_matches('/').split('/').last().unwrap_or("").to_string()
                } else {
                    href.split('/').last().unwrap_or("").to_string()
                };
                
                if !name.is_empty() {
                    items.push(WebDAVItem {
                        name,
                        path: href,
                        is_dir: is_collection,
                        size,
                        modified,
                    });
                }
            }
        }
        
        i += 1;
    }
    
    items
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
