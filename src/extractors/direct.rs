use async_trait::async_trait;
use reqwest::Client;
use std::path::Path;
use url::Url;

use super::{AudioFormat, AudioInfo, MediaExtractor};
use crate::Result;

/// Direct URL extractor for audio and video files
pub struct DirectExtractor {
    client: Client,
}

impl DirectExtractor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
    
    /// Determine audio format from URL or content type
    fn determine_format(&self, url: &str, content_type: Option<&str>) -> AudioFormat {
        // Try to determine from URL extension first
        if let Ok(parsed_url) = Url::parse(url) {
            if let Some(path) = parsed_url.path_segments() {
                if let Some(filename) = path.last() {
                    if let Some(extension) = Path::new(filename).extension() {
                        if let Some(format) = AudioFormat::from_extension(&extension.to_string_lossy()) {
                            return format;
                        }
                    }
                }
            }
        }
        
        // Try to determine from content type
        if let Some(content_type) = content_type {
            match content_type {
                ct if ct.contains("mp3") || ct.contains("mpeg") => return AudioFormat::Mp3,
                ct if ct.contains("mp4") || ct.contains("m4a") => return AudioFormat::M4a,
                ct if ct.contains("wav") => return AudioFormat::Wav,
                ct if ct.contains("flac") => return AudioFormat::Flac,
                ct if ct.contains("ogg") => return AudioFormat::Ogg,
                ct if ct.contains("webm") => return AudioFormat::Webm,
                _ => {}
            }
        }
        
        // Default to MP3
        AudioFormat::Mp3
    }
    
    /// Check if URL points to an audio or video file
    fn is_media_url(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();
        
        // Check for common audio/video extensions
        let media_extensions = [
            ".mp3", ".m4a", ".wav", ".flac", ".ogg", ".aac",
            ".mp4", ".avi", ".mov", ".mkv", ".webm", ".m4v"
        ];
        
        media_extensions.iter().any(|ext| url_lower.contains(ext))
    }
    
    /// Get content information via HEAD request
    async fn get_content_info(&self, url: &str) -> Result<(Option<String>, Option<u64>)> {
        let response = self.client.head(url).send().await?;
        
        if !response.status().is_success() {
            anyhow::bail!("Failed to access URL: HTTP {}", response.status());
        }
        
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|ct| ct.to_str().ok())
            .map(|s| s.to_string());
            
        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|cl| cl.to_str().ok())
            .and_then(|cl| cl.parse::<u64>().ok());
            
        Ok((content_type, content_length))
    }
}

#[async_trait]
impl MediaExtractor for DirectExtractor {
    async fn extract_audio_info(&self, url: &str) -> Result<AudioInfo> {
        // Validate URL
        let parsed_url = Url::parse(url)
            .map_err(|_| anyhow::anyhow!("Invalid URL: {}", url))?;
            
        // Get content information
        let (content_type, file_size) = self.get_content_info(url).await?;
        
        // Determine format
        let format = self.determine_format(url, content_type.as_deref());
        
        // Extract title from filename
        let title = parsed_url
            .path_segments()
            .and_then(|segments| segments.last())
            .filter(|filename| !filename.is_empty())
            .map(|filename| {
                // Remove extension and decode URL encoding
                let name = if let Some(dot_pos) = filename.rfind('.') {
                    &filename[..dot_pos]
                } else {
                    filename
                };
                urlencoding::decode(name)
                    .unwrap_or_else(|_| name.into())
                    .replace(['_', '-'], " ")
            });
        
        Ok(AudioInfo {
            download_url: url.to_string(),
            duration: None, // Can't determine without downloading
            title,
            format,
            sample_rate: None, // Unknown without analysis
            file_size,
            original_url: url.to_string(),
        })
    }
    
    fn supports_url(&self, url: &str) -> bool {
        // Parse URL to ensure it's valid
        if Url::parse(url).is_err() {
            return false;
        }
        
        // Check if it looks like a media file
        self.is_media_url(url)
    }
    
    fn platform_name(&self) -> &'static str {
        "Direct URL"
    }
}

impl Default for DirectExtractor {
    fn default() -> Self {
        Self::new()
    }
} 