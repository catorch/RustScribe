use anyhow::{Context, Result};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_transcribe::Client as TranscribeClient;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

use crate::config::Config;
use crate::extractors::{AudioInfo, ExtractorRegistry};

pub mod processor;

/// Transcription result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// The transcribed text
    pub transcript: String,
    
    /// Segments with timestamps (if available)
    pub segments: Vec<TranscriptSegment>,
    
    /// Original audio information
    pub audio_info: AudioInfo,
    
    /// Path to downloaded audio file (if preserved)
    pub audio_path: Option<PathBuf>,
    
    /// Transcription metadata
    pub metadata: TranscriptionMetadata,
}

/// Individual transcript segment with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Start time in seconds
    pub start_time: f64,
    
    /// End time in seconds
    pub end_time: f64,
    
    /// Segment text
    pub text: String,
    
    /// Confidence score (0.0 to 1.0)
    pub confidence: Option<f64>,
    
    /// Speaker ID (if speaker identification is enabled)
    pub speaker_id: Option<String>,
}

/// Metadata about the transcription process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionMetadata {
    /// AWS Transcribe job ID
    pub job_id: String,
    
    /// Language detected/used
    pub language: String,
    
    /// Processing time in seconds
    pub processing_duration: Option<f64>,
    
    /// Audio duration in seconds
    pub audio_duration: Option<f64>,
    
    /// Overall confidence score
    pub confidence: Option<f64>,
    
    /// Timestamp when transcription completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Main transcription pipeline
pub struct TranscriptionPipeline {
    config: Config,
    extractor_registry: ExtractorRegistry,
    s3_client: S3Client,
    transcribe_client: TranscribeClient,
    temp_dir: TempDir,
}

impl TranscriptionPipeline {
    /// Create a new transcription pipeline
    pub async fn new(config: Config) -> Result<Self> {
        // Load AWS configuration
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(config.aws_region())
            .load()
            .await;
            
        let s3_client = S3Client::new(&aws_config);
        let transcribe_client = TranscribeClient::new(&aws_config);
        
        // Create temporary directory
        let temp_dir = TempDir::new()
            .context("Failed to create temporary directory")?;
        
        Ok(Self {
            config,
            extractor_registry: ExtractorRegistry::new(),
            s3_client,
            transcribe_client,
            temp_dir,
        })
    }
    
    /// Transcribe audio from a URL
    pub async fn transcribe_from_url(
        &self,
        url: &str,
        language: Option<&str>,
        speaker_labels: bool,
        max_speakers: Option<u8>,
        max_segment_length: f64,
    ) -> Result<TranscriptionResult> {
        // Extract audio information
        tracing::info!("Extracting audio information from URL: {}", url);
        let audio_info = self.extractor_registry.extract_audio_info(url).await?;
        
        // Download audio file
        let audio_path = self.download_audio(&audio_info).await?;
        
        // Upload to S3
        let s3_key = self.upload_to_s3(&audio_path, &audio_info).await?;
        
        // Start transcription job
        let job_id = self.start_transcription_job(&s3_key, &audio_info, language, speaker_labels, max_speakers).await?;
        
        // Wait for completion
        let result = self.wait_for_transcription(&job_id, max_segment_length).await?;
        
        // Clean up S3 object
        self.cleanup_s3(&s3_key).await?;
        
        // Preserve audio file if configured
        let preserved_audio_path = if self.config.app.keep_audio {
            Some(self.preserve_audio_file(&audio_path, &audio_info).await?)
        } else {
            None
        };
        
        Ok(TranscriptionResult {
            transcript: result.transcript,
            segments: result.segments,
            audio_info,
            audio_path: preserved_audio_path,
            metadata: result.metadata,
        })
    }
    
    /// Download audio file to temporary location
    async fn download_audio(&self, audio_info: &AudioInfo) -> Result<PathBuf> {
        let filename = format!(
            "audio_{}.{}",
            &Uuid::new_v4().to_string()[..8],
            audio_info.format.as_str()
        );
        let audio_path = self.temp_dir.path().join(filename);
        
        tracing::info!("Downloading audio to: {}", audio_path.display());
        
        // Check if this is a YouTube URL (yt-dlp protocol)
        if audio_info.download_url.starts_with("yt-dlp://") {
            // Use optimized YouTube download
            let youtube_url = &audio_info.download_url[9..]; // Remove "yt-dlp://" prefix
            let youtube_extractor = crate::extractors::youtube::YoutubeExtractor::new();
            
            let progress = ProgressBar::new_spinner();
            progress.set_style(ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap()
            );
            progress.set_message("Downloading audio with yt-dlp (optimized)...");
            
            // Let yt-dlp handle the download directly (much faster!)
            youtube_extractor.download_audio_direct(youtube_url, &audio_path).await?;
            
            progress.finish_with_message("Download complete");
            return Ok(audio_path);
        }
        
        // Create progress bar for regular downloads
        let progress = ProgressBar::new(audio_info.file_size.unwrap_or(0));
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}")
                .unwrap()
        );
        progress.set_message("Downloading audio...");
        
        // Download with progress tracking for non-YouTube URLs
        let response = reqwest::get(&audio_info.download_url).await?;
        
        if !response.status().is_success() {
            anyhow::bail!("Failed to download audio: HTTP {}", response.status());
        }
        
        let total_size = response.content_length().unwrap_or(0);
        progress.set_length(total_size);
        
        let mut file = fs_err::File::create(&audio_path)?;
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        
        use futures_util::StreamExt;
        use std::io::Write;
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            progress.set_position(downloaded);
        }
        
        progress.finish_with_message("Download complete");
        
        Ok(audio_path)
    }
    
    /// Upload audio file to S3
    async fn upload_to_s3(&self, audio_path: &PathBuf, audio_info: &AudioInfo) -> Result<String> {
        let key = format!(
            "{}audio_{}_{}.{}",
            self.config.aws.s3_key_prefix.as_deref().unwrap_or(""),
            Uuid::new_v4(),
            chrono::Utc::now().format("%Y%m%d_%H%M%S"),
            audio_info.format.as_str()
        );
        
        tracing::info!("Uploading audio to S3: s3://{}/{}", self.config.aws.s3_bucket, key);
        
        let content = fs_err::read(audio_path)?;
        
        self.s3_client
            .put_object()
            .bucket(&self.config.aws.s3_bucket)
            .key(&key)
            .body(content.into())
            .content_type(audio_info.format.mime_type())
            .send()
            .await
            .context("Failed to upload audio to S3")?;
            
        Ok(key)
    }
    
    /// Start AWS Transcribe job with auto language detection and speaker identification
    async fn start_transcription_job(
        &self,
        s3_key: &str,
        audio_info: &AudioInfo,
        language: Option<&str>,
        speaker_labels: bool,
        max_speakers: Option<u8>,
    ) -> Result<String> {
        let job_name = format!("transcriptor_{}", Uuid::new_v4());
        let media_uri = format!("s3://{}/{}", self.config.aws.s3_bucket, s3_key);
        
        tracing::info!("Starting transcription job: {}", job_name);
        
        use aws_sdk_transcribe::types::{Media, MediaFormat, Settings};
        
        let media_format = match audio_info.format {
            crate::extractors::AudioFormat::Mp3 => MediaFormat::Mp3,
            crate::extractors::AudioFormat::M4a => MediaFormat::Mp4,
            crate::extractors::AudioFormat::Wav => MediaFormat::Wav,
            crate::extractors::AudioFormat::Flac => MediaFormat::Flac,
            crate::extractors::AudioFormat::Ogg => MediaFormat::Ogg,
            crate::extractors::AudioFormat::Webm => MediaFormat::Webm,
        };
        
        let media = Media::builder()
            .media_file_uri(media_uri)
            .build();
        
        let mut job_builder = self.transcribe_client
            .start_transcription_job()
            .transcription_job_name(&job_name)
            .media_format(media_format)
            .media(media);
        
        // Handle language detection
        if let Some(lang) = language.or(self.config.aws.transcription.default_language.as_deref()) {
            tracing::info!("Using specified language: {}", lang);
            job_builder = job_builder.language_code(lang.parse()?);
        } else {
            tracing::info!("Using automatic language detection");
            job_builder = job_builder.identify_language(true);
        }
        
        // Add sample rate to job builder
        if let Some(sample_rate) = audio_info.sample_rate {
            job_builder = job_builder.media_sample_rate_hertz(sample_rate as i32);
        }
        
        // Add optional settings for speaker identification and word-level timestamps
        let mut settings = Settings::builder();
        
        // Enable word-level timestamps for more granular segments
        tracing::info!("Enabling word-level timestamps for better granularity");
        settings = settings.show_alternatives(true);
        settings = settings.max_alternatives(2); // AWS requires minimum of 2
        
        // Configure speaker identification
        let enable_speaker_id = speaker_labels || self.config.aws.transcription.speaker_identification;
        if enable_speaker_id {
            tracing::info!("Enabling speaker identification");
            settings = settings.show_speaker_labels(true);
            
            // Set max speakers (AWS supports 2-10 speakers)
            let max_speakers_count = max_speakers
                .or(self.config.aws.transcription.max_speakers.map(|s| s as u8))
                .unwrap_or(10); // Default to 10 if not specified
                
            let clamped_speakers = max_speakers_count.clamp(2, 10);
            settings = settings.max_speaker_labels(clamped_speakers as i32);
            
            if max_speakers_count != clamped_speakers {
                tracing::warn!("Max speakers clamped from {} to {} (AWS supports 2-10)", max_speakers_count, clamped_speakers);
            }
        }
        
        job_builder = job_builder.settings(settings.build());
        
        job_builder.send().await
            .context("Failed to start transcription job")?;
            
        Ok(job_name)
    }
    
    /// Wait for transcription job completion
    async fn wait_for_transcription(&self, job_id: &str, max_segment_length: f64) -> Result<processor::ProcessedTranscription> {
        processor::TranscriptionProcessor::new(
            self.transcribe_client.clone(),
            job_id.to_string(),
            max_segment_length,
        )
        .wait_for_completion()
        .await
    }
    
    /// Clean up S3 object
    async fn cleanup_s3(&self, s3_key: &str) -> Result<()> {
        tracing::debug!("Cleaning up S3 object: {}", s3_key);
        
        self.s3_client
            .delete_object()
            .bucket(&self.config.aws.s3_bucket)
            .key(s3_key)
            .send()
            .await
            .context("Failed to clean up S3 object")?;
            
        Ok(())
    }
    
    /// Preserve audio file in user's directory
    async fn preserve_audio_file(
        &self,
        temp_path: &PathBuf,
        audio_info: &AudioInfo,
    ) -> Result<PathBuf> {
        let filename = audio_info
            .title
            .as_ref()
            .map(|title| {
                let sanitized = title
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' })
                    .collect::<String>();
                format!("{}.{}", sanitized, audio_info.format.as_str())
            })
            .unwrap_or_else(|| {
                format!(
                    "audio_{}.{}",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S"),
                    audio_info.format.as_str()
                )
            });
            
        let output_path = std::env::current_dir()?.join(filename);
        fs_err::copy(temp_path, &output_path)?;
        
        Ok(output_path)
    }
} 