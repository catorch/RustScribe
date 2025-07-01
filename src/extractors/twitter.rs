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
    
    /// Download audio directly using yt-dlp (similar to YouTube approach)
    pub async fn download_audio_direct(&self, url: &str, output_path: &std::path::Path) -> Result<AudioFormat> {
        tracing::debug!("Downloading Twitter audio directly for: {}", url);
        
        let output = Command::new(&self.yt_dlp_path)
            .args([
                // Output to specific file
                "--output", &output_path.to_string_lossy(),
                // Extract audio in the most efficient format for transcription
                "--extract-audio",
                "--audio-format", "mp3",
                "--audio-quality", "9",  // Lowest quality for speed (still good for transcription)
                // Better Twitter audio selection
                "--format", "hls-audio-32000-Audio/bestaudio[ext=m4a]/bestaudio[ext=mp4]/bestaudio/best[height<=720]",
                "--no-playlist",
                // Performance optimizations
                "--concurrent-fragments", "4",
                "--newline",
                url,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            
            // Check for common Twitter errors
            if error.contains("No video could be found") {
                anyhow::bail!("This tweet does not contain any video or audio content");
            } else if error.contains("Private") || error.contains("protected") {
                anyhow::bail!("This tweet is private or protected");
            } else if error.contains("not found") || error.contains("404") {
                anyhow::bail!("Tweet not found or has been deleted");
            }
            
            anyhow::bail!("Failed to download audio from Twitter: {}", error);
        }
        
        Ok(AudioFormat::Mp3) // We're forcing MP3 conversion for speed
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
        
        // For Twitter, we'll use direct download, so we use a placeholder URL
        // The actual download will be handled by download_audio_direct()
        let download_url = format!("twitter-dlp://{}", url);
        
        // We'll always convert to MP3 for speed and compatibility
        let format = AudioFormat::Mp3;
        
        Ok(AudioInfo {
            download_url,
            duration,
            title,
            format,
            sample_rate: Some(44100),
            file_size: None, // Will be determined during download
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