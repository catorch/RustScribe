use async_trait::async_trait;
use chrono::Duration;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

use super::{AudioFormat, AudioInfo, MediaExtractor};
use crate::Result;

/// YouTube audio extractor using yt-dlp
pub struct YoutubeExtractor {
    yt_dlp_path: String,
}

impl YoutubeExtractor {
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
    
    /// Get video information using yt-dlp
    async fn get_video_info(&self, url: &str) -> Result<Value> {
        tracing::debug!("Extracting video info for: {}", url);
        
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
            anyhow::bail!("yt-dlp failed: {}", error);
        }
        
        let json_str = String::from_utf8(output.stdout)?;
        let info: Value = serde_json::from_str(&json_str)?;
        
        Ok(info)
    }
    
    /// Download audio directly using yt-dlp (much faster than URL extraction + separate download)
    pub async fn download_audio_direct(&self, url: &str, output_path: &std::path::Path) -> Result<AudioFormat> {
        tracing::debug!("Downloading audio directly for: {}", url);
        
        let output = Command::new(&self.yt_dlp_path)
            .args([
                // Output to specific file
                "--output", &output_path.to_string_lossy(),
                // Extract audio in the most efficient format for transcription
                "--extract-audio",
                "--audio-format", "mp3",
                "--audio-quality", "9",  // Lowest quality for speed (still good for transcription)
                // Prioritize smaller/faster formats
                "--format", "worstaudio[acodec^=mp4a]/worstaudio[ext=m4a]/worstaudio[ext=mp3]/worstaudio",
                "--no-playlist",
                // Performance optimizations
                "--concurrent-fragments", "4",
                "--throttled-rate", "100K",
                "--newline",
                url,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
            
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to download audio: {}", error);
        }
        
        Ok(AudioFormat::Mp3) // We're forcing MP3 conversion for speed
    }
}

#[async_trait]
impl MediaExtractor for YoutubeExtractor {
    async fn extract_audio_info(&self, url: &str) -> Result<AudioInfo> {
        // Check if yt-dlp is available
        if !self.check_availability().await? {
            anyhow::bail!("yt-dlp is not available. Please install it: https://github.com/yt-dlp/yt-dlp");
        }
        
        // Get video information
        let info = self.get_video_info(url).await?;
        
        // Extract metadata
        let title = info["title"].as_str().map(|s| s.to_string());
        let duration_seconds = info["duration"].as_f64();
        let duration = duration_seconds.map(|d| Duration::seconds(d as i64));
        
        // For YouTube, we'll use direct download, so we use a placeholder URL
        // The actual download will be handled by download_audio_direct()
        let download_url = format!("yt-dlp://{}", url);
        
        // We'll always convert to MP3 for speed and compatibility
        let format = AudioFormat::Mp3;
        
        Ok(AudioInfo {
            download_url,
            duration,
            title,
            format,
            sample_rate: Some(44100), // YouTube typically uses 44.1kHz  
            file_size: None, // Will be determined during download
            original_url: url.to_string(),
        })
    }
    
    fn supports_url(&self, url: &str) -> bool {
        // Support various YouTube URL formats
        let url_lower = url.to_lowercase();
        url_lower.contains("youtube.com/watch") ||
        url_lower.contains("youtu.be/") ||
        url_lower.contains("youtube.com/embed/") ||
        url_lower.contains("youtube.com/v/") ||
        url_lower.contains("m.youtube.com/")
    }
    
    fn platform_name(&self) -> &'static str {
        "YouTube"
    }
}

impl Default for YoutubeExtractor {
    fn default() -> Self {
        Self::new()
    }
} 