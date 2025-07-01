use async_trait::async_trait;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

pub mod youtube;
pub mod twitter;
pub mod direct;
pub mod local;

use crate::Result;

/// Information about extracted audio
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInfo {
    /// Direct download URL for the audio
    pub download_url: String,
    
    /// Duration of the audio if available
    pub duration: Option<Duration>,
    
    /// Title or description of the media
    pub title: Option<String>,
    
    /// Audio format (mp3, m4a, wav, etc.)
    pub format: AudioFormat,
    
    /// Sample rate in Hz
    pub sample_rate: Option<u32>,
    
    /// File size in bytes if available
    pub file_size: Option<u64>,
    
    /// Original URL that was processed
    pub original_url: String,
}

/// Supported audio formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AudioFormat {
    Mp3,
    M4a,
    Wav,
    Flac,
    Ogg,
    Webm,
}

impl AudioFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Wav => "wav",
            AudioFormat::Flac => "flac",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Webm => "webm",
        }
    }
    
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "mp3" => Some(AudioFormat::Mp3),
            "m4a" | "aac" => Some(AudioFormat::M4a),
            "wav" => Some(AudioFormat::Wav),
            "flac" => Some(AudioFormat::Flac),
            "ogg" => Some(AudioFormat::Ogg),
            "webm" => Some(AudioFormat::Webm),
            _ => None,
        }
    }
    
    /// Get MIME type for the format
    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::M4a => "audio/mp4",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Webm => "audio/webm",
        }
    }
}

/// Trait for extracting audio from different platforms
#[async_trait]
pub trait MediaExtractor: Send + Sync {
    /// Extract audio information from a URL
    async fn extract_audio_info(&self, url: &str) -> Result<AudioInfo>;
    
    /// Check if this extractor supports the given URL
    fn supports_url(&self, url: &str) -> bool;
    
    /// Get the name of this platform
    fn platform_name(&self) -> &'static str;
    
    /// Download audio to a temporary file
    async fn download_audio(&self, audio_info: &AudioInfo, output_path: &PathBuf) -> Result<()> {
        let response = reqwest::get(&audio_info.download_url).await?;
        
        if !response.status().is_success() {
            anyhow::bail!("Failed to download audio: HTTP {}", response.status());
        }
        
        let content = response.bytes().await?;
        fs_err::write(output_path, content)?;
        
        Ok(())
    }
}

/// Registry for managing multiple extractors
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn MediaExtractor>>,
}

impl ExtractorRegistry {
    /// Create a new registry with default extractors
    pub fn new() -> Self {
        let mut registry = Self {
            extractors: Vec::new(),
        };
        
        // Register default extractors
        registry.register(Box::new(youtube::YoutubeExtractor::new()));
        registry.register(Box::new(twitter::TwitterExtractor::new()));
        registry.register(Box::new(direct::DirectExtractor::new()));
        
        registry
    }
    
    /// Create local file extractor (not stored in registry since it's handled differently)
    pub fn create_local_extractor() -> local::LocalFileExtractor {
        local::LocalFileExtractor::new()
    }
    
    /// Register a new extractor
    pub fn register(&mut self, extractor: Box<dyn MediaExtractor>) {
        self.extractors.push(extractor);
    }
    
    /// Find an extractor that supports the given URL
    pub fn find_extractor(&self, url: &str) -> Option<&dyn MediaExtractor> {
        self.extractors
            .iter()
            .find(|extractor| extractor.supports_url(url))
            .map(|boxed| boxed.as_ref())
    }
    
    /// List all supported platforms
    pub fn list_platforms(&self) -> Vec<&'static str> {
        self.extractors
            .iter()
            .map(|extractor| extractor.platform_name())
            .collect()
    }
    
    /// Check if input is a local file path
    pub fn is_local_file(&self, input: &str) -> bool {
        // First, check if it's clearly a URL
        if input.starts_with("http://") || input.starts_with("https://") {
            return false;
        }
        
        // Check if the file exists (handles both absolute and relative paths)
        let path = std::path::Path::new(input);
        if path.exists() {
            return true;
        }
        
        // Check if it looks like a file path (has file extension or path separators)
        let has_extension = path.extension().is_some();
        let has_path_separators = input.contains('/') || input.contains('\\');
        let starts_with_dot = input.starts_with("./") || input.starts_with(".\\");
        
        has_extension || has_path_separators || starts_with_dot
    }
    
    /// Extract audio info using the appropriate extractor
    pub async fn extract_audio_info(&self, input: &str) -> Result<AudioInfo> {
        // Check if it's a local file
        if self.is_local_file(input) {
            let local_extractor = Self::create_local_extractor();
            return local_extractor.extract_audio_info(input).await;
        }
        
        // Handle as URL
        let extractor = self
            .find_extractor(input)
            .ok_or_else(|| anyhow::anyhow!("No extractor found for URL: {}", input))?;
        
        extractor.extract_audio_info(input).await
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate and normalize URLs
pub fn validate_url(url: &str) -> Result<Url> {
    let parsed = Url::parse(url)
        .map_err(|_| anyhow::anyhow!("Invalid URL format: {}", url))?;
    
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("URL must use HTTP or HTTPS protocol");
    }
    
    Ok(parsed)
} 