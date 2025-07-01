//! Universal Transcriptor - A Rust CLI tool for transcribing media from various platforms
//! 
//! This library provides functionality to extract and transcribe audio from platforms like
//! YouTube, Twitter/X, and direct media URLs using AWS Transcribe service.

pub mod cli;
pub mod config;
pub mod extractors;
pub mod output;
pub mod transcribe;
pub mod utils;

pub use cli::{Cli, Commands, OutputFormat};
pub use config::Config;
pub use extractors::{AudioInfo, MediaExtractor};
pub use transcribe::{TranscriptionPipeline, TranscriptionResult};

/// Result type used throughout the library
pub type Result<T> = anyhow::Result<T>;

/// Error types specific to the transcriptor
#[derive(thiserror::Error, Debug)]
pub enum TranscriptorError {
    #[error("Unsupported URL format: {0}")]
    UnsupportedUrl(String),
    
    #[error("Audio extraction failed: {0}")]
    AudioExtractionFailed(String),
    
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),
    
    #[error("AWS configuration error: {0}")]
    AwsConfigError(String),
    
    #[error("File operation failed: {0}")]
    FileError(String),
} 