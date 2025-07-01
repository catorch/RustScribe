use anyhow::{Context, Result};
use aws_config::Region;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// AWS configuration
    pub aws: AwsConfig,
    
    /// Application settings
    pub app: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsConfig {
    /// AWS region
    pub region: String,
    
    /// S3 bucket for temporary audio storage
    pub s3_bucket: String,
    
    /// Optional S3 key prefix
    pub s3_key_prefix: Option<String>,
    
    /// Transcription job settings
    pub transcription: TranscriptionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Default language code (if not specified)
    pub default_language: Option<String>,
    
    /// Media format preference
    pub media_format: String,
    
    /// Sample rate for audio processing
    pub sample_rate: Option<u32>,
    
    /// Enable speaker identification
    pub speaker_identification: bool,
    
    /// Maximum speakers for identification
    pub max_speakers: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Temporary directory for downloads
    pub temp_dir: Option<PathBuf>,
    
    /// Keep audio files after transcription
    pub keep_audio: bool,
    
    /// Default output format
    pub default_output_format: String,
    
    /// Maximum concurrent jobs
    pub max_concurrent_jobs: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aws: AwsConfig {
                region: "us-east-1".to_string(),
                s3_bucket: "".to_string(),
                s3_key_prefix: Some("transcriptor/".to_string()),
                transcription: TranscriptionConfig {
                    default_language: None,
                    media_format: "mp3".to_string(),
                    sample_rate: Some(16000),
                    speaker_identification: false,
                    max_speakers: None,
                },
            },
            app: AppConfig {
                temp_dir: None,
                keep_audio: false,
                default_output_format: "text".to_string(),
                max_concurrent_jobs: 3,
            },
        }
    }
}

impl Config {
    /// Load configuration from file or create default
    pub async fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        
        if config_path.exists() {
            let content = fs_err::read_to_string(&config_path)
                .context("Failed to read config file")?;
            
            let config: Config = serde_yaml::from_str(&content)
                .context("Failed to parse config file")?;
            
            config.validate()?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save().await?;
            Ok(config)
        }
    }
    
    /// Save configuration to file
    pub async fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        
        if let Some(parent) = config_path.parent() {
            fs_err::create_dir_all(parent)?;
        }
        
        let content = serde_yaml::to_string(self)
            .context("Failed to serialize config")?;
        
        fs_err::write(&config_path, content)
            .context("Failed to write config file")?;
        
        Ok(())
    }
    
    /// Get configuration file path
    fn config_path() -> Result<PathBuf> {
        // First try current directory for easy testing
        let local_config = PathBuf::from("config.yaml");
        if local_config.exists() {
            return Ok(local_config);
        }
        
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?;
        
        Ok(config_dir.join("universal-transcriptor").join("config.yaml"))
    }
    
    /// Validate configuration
    fn validate(&self) -> Result<()> {
        if self.aws.s3_bucket.is_empty() {
            anyhow::bail!("AWS S3 bucket must be configured");
        }
        
        Region::new(self.aws.region.clone());
        
        Ok(())
    }
    
    /// Display current configuration
    pub fn display(&self) {
        println!("Current Configuration:");
        println!("  AWS Region: {}", self.aws.region);
        println!("  S3 Bucket: {}", self.aws.s3_bucket);
        if let Some(prefix) = &self.aws.s3_key_prefix {
            println!("  S3 Prefix: {}", prefix);
        }
        println!("  Keep Audio: {}", self.app.keep_audio);
        println!("  Default Format: {}", self.app.default_output_format);
    }
    
    /// Interactive configuration setup
    pub async fn interactive_setup(&self) -> Result<()> {
        println!("Interactive configuration setup coming soon!");
        println!("For now, please edit the config file manually:");
        println!("  {}", Self::config_path()?.display());
        Ok(())
    }
    
    /// Get AWS region
    pub fn aws_region(&self) -> Region {
        Region::new(self.aws.region.clone())
    }
} 