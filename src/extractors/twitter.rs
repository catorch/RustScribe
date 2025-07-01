use async_trait::async_trait;
use chrono::Duration;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

use super::{AudioFormat, AudioInfo, MediaExtractor};
use crate::Result;

/// Twitter/X audio extractor using yt-dlp
pub struct TwitterExtractor {
    yt_dlp_path: String,
}

impl TwitterExtractor {
    pub fn new() -> Self {
        Self {
            yt_dlp_path: "yt-dlp".to_string(),
        }
    }
    
    /// Check if yt-dlp is available
    pub async fn check_availability(&self) -> Result<bool> {
        let output = Command::new(&self.yt_dlp_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
            
        Ok(output.is_ok() && output.unwrap().status.success())
    }
    
    /// Get tweet information using yt-dlp
    async fn get_tweet_info(&self, url: &str) -> Result<Value> {
        tracing::debug!("Extracting tweet info for: {}", url);
        
        let output = Command::new(&self.yt_dlp_path)
            .args([
                "--dump-json",
                "--no-playlist",
                url,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("yt-dlp failed to extract Twitter content: {}", error);
        }
        
        let json_str = String::from_utf8(output.stdout)?;
        let info: Value = serde_json::from_str(&json_str)?;
        
        Ok(info)
    }
    
    /// Extract the best audio format URL
    async fn get_audio_url(&self, url: &str) -> Result<String> {
        tracing::debug!("Getting audio URL for Twitter content: {}", url);
        
        let output = Command::new(&self.yt_dlp_path)
            .args([
                "--get-url",
                "--format", "bestaudio/best",
                "--no-playlist",
                url,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get audio URL from Twitter: {}", error);
        }
        
        let audio_url = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
            
        Ok(audio_url)
    }
}

#[async_trait]
impl MediaExtractor for TwitterExtractor {
    async fn extract_audio_info(&self, url: &str) -> Result<AudioInfo> {
        // Check if yt-dlp is available
        if !self.check_availability().await? {
            anyhow::bail!("yt-dlp is not available. Please install it: https://github.com/yt-dlp/yt-dlp");
        }
        
        // Get tweet information
        let info = self.get_tweet_info(url).await?;
        
        // Extract metadata
        let title = info["description"]
            .as_str()
            .or_else(|| info["title"].as_str())
            .map(|s| {
                // Truncate long descriptions and clean up
                let cleaned = s.replace('\n', " ").trim().to_string();
                if cleaned.len() > 100 {
                    format!("{}...", &cleaned[..97])
                } else {
                    cleaned
                }
            });
            
        let duration_seconds = info["duration"].as_f64();
        let duration = duration_seconds.map(|d| Duration::seconds(d as i64));
        
        // Get the audio download URL
        let download_url = self.get_audio_url(url).await?;
        
        // Twitter videos are typically MP4, so we'll extract audio as MP3
        let format = if download_url.contains(".m4a") {
            AudioFormat::M4a
        } else {
            AudioFormat::Mp3 // Default conversion for Twitter
        };
        
        Ok(AudioInfo {
            download_url,
            duration,
            title,
            format,
            sample_rate: Some(44100),
            file_size: None,
            original_url: url.to_string(),
        })
    }
    
    fn supports_url(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();
        
        // Support various Twitter/X URL formats
        url_lower.contains("twitter.com/") ||
        url_lower.contains("x.com/") ||
        url_lower.contains("mobile.twitter.com/") ||
        url_lower.contains("m.twitter.com/")
    }
    
    fn platform_name(&self) -> &'static str {
        "Twitter/X"
    }
}

impl Default for TwitterExtractor {
    fn default() -> Self {
        Self::new()
    }
} 