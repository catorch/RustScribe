use super::{AudioFormat, AudioInfo, MediaExtractor};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Duration;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::{Command};

pub struct LocalFileExtractor;

impl LocalFileExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Check if the file exists and is accessible
    async fn validate_file(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            anyhow::bail!("File does not exist: {}", path.display());
        }

        if !path.is_file() {
            anyhow::bail!("Path is not a file: {}", path.display());
        }

        // Check if file is readable
        match fs::metadata(path).await {
            Ok(metadata) => {
                if metadata.len() == 0 {
                    anyhow::bail!("File is empty: {}", path.display());
                }
            }
            Err(e) => {
                anyhow::bail!("Cannot access file {}: {}", path.display(), e);
            }
        }

        Ok(())
    }

    /// Get file information using ffprobe
    async fn get_file_info(&self, path: &Path) -> Result<(Option<f64>, String)> {
        let output = Command::new("ffprobe")
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                &path.to_string_lossy(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to analyze file with ffprobe: {}", error);
        }

        let info: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        
        // Extract duration
        let duration = info["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse::<f64>().ok());

        // Extract title/filename
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Local File")
            .to_string();

        // Check if file has audio streams
        let empty_vec = vec![];
        let streams = info["streams"].as_array().unwrap_or(&empty_vec);
        let has_audio = streams.iter().any(|stream| {
            stream["codec_type"].as_str() == Some("audio")
        });

        if !has_audio {
            anyhow::bail!("File does not contain any audio streams: {}", path.display());
        }

        Ok((duration, title))
    }

    /// Determine audio format from file extension
    fn get_audio_format(&self, path: &Path) -> AudioFormat {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("mp3") => AudioFormat::Mp3,
            Some("m4a") | Some("aac") => AudioFormat::M4a,
            Some("wav") => AudioFormat::Wav,
            Some("flac") => AudioFormat::Flac,
            Some("ogg") => AudioFormat::Ogg,
            // For video files or unknown formats, we'll assume they need conversion
            _ => AudioFormat::Mp3, // Default to MP3 for compatibility
        }
    }

    /// Copy or convert local file to the target path
    pub async fn prepare_audio(&self, source_path: &Path, target_path: &Path) -> Result<AudioFormat> {
        tracing::debug!("Preparing local audio file: {} -> {}", source_path.display(), target_path.display());

        let source_format = self.get_audio_format(source_path);
        
        // Check if it's already an audio file in a good format
        let is_audio_file = matches!(source_path.extension().and_then(|ext| ext.to_str()), 
            Some("mp3") | Some("m4a") | Some("aac") | Some("wav") | Some("flac") | Some("ogg"));

        if is_audio_file {
            // For pure audio files, we can often just copy them
            match source_format {
                AudioFormat::Mp3 | AudioFormat::M4a => {
                    // These formats work well with AWS Transcribe, just copy
                    tokio::fs::copy(source_path, target_path).await?;
                    Ok(source_format)
                }
                _ => {
                    // Convert other audio formats to MP3 for compatibility
                    self.convert_to_mp3(source_path, target_path).await?;
                    Ok(AudioFormat::Mp3)
                }
            }
        } else {
            // For video files or unknown formats, extract/convert to MP3
            self.convert_to_mp3(source_path, target_path).await?;
            Ok(AudioFormat::Mp3)
        }
    }

    /// Convert file to MP3 using ffmpeg
    async fn convert_to_mp3(&self, source_path: &Path, target_path: &Path) -> Result<()> {
        tracing::debug!("Converting {} to MP3", source_path.display());

        let output = Command::new("ffmpeg")
            .args([
                "-i", &source_path.to_string_lossy(),
                "-vn", // No video
                "-acodec", "mp3",
                "-ab", "128k", // Good quality for transcription
                "-ar", "44100", // Standard sample rate
                "-y", // Overwrite output file
                &target_path.to_string_lossy(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to convert file with ffmpeg: {}", error);
        }

        Ok(())
    }
}

#[async_trait]
impl MediaExtractor for LocalFileExtractor {
    async fn extract_audio_info(&self, path: &str) -> Result<AudioInfo> {
        let file_path = Path::new(path);
        
        // Validate the file exists and is accessible
        self.validate_file(file_path).await?;

        // Get file information
        let (duration_seconds, title) = self.get_file_info(file_path).await?;
        let duration = duration_seconds.map(|d| Duration::seconds(d as i64));
        
        // Get file size
        let metadata = fs::metadata(file_path).await?;
        let file_size = Some(metadata.len());

        // Determine format
        let format = self.get_audio_format(file_path);

        // For local files, we'll use a special protocol with absolute path to avoid path issues
        let absolute_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
        let download_url = format!("local-file://{}", absolute_path.display());

        Ok(AudioInfo {
            download_url,
            duration,
            title: Some(title),
            format,
            sample_rate: Some(44100), // Will be normalized to this
            file_size,
            original_url: path.to_string(),
        })
    }

    fn supports_url(&self, _url: &str) -> bool {
        // This extractor doesn't support URLs, only local files
        // Local files are handled separately in the ExtractorRegistry
        false
    }

    fn platform_name(&self) -> &'static str {
        "Local File"
    }

    async fn download_audio(&self, _audio_info: &AudioInfo, _output_path: &PathBuf) -> Result<()> {
        // This method won't be called for local files since we handle it differently
        Err(anyhow!("Local files use direct processing, not download"))
    }
} 