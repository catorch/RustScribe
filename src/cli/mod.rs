use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "transcriptor",
    about = "Universal Transcriptor - Extract transcripts from YouTube, Twitter, and more using AWS Transcribe",
    version,
    long_about = "A powerful CLI tool for transcribing audio from various platforms including YouTube, Twitter/X, and direct media URLs. Uses AWS Transcribe for high-quality speech-to-text conversion."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Disable progress indicators
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Transcribe audio from a URL or local file
    Transcribe {
        /// URL or file path to transcribe (YouTube, Twitter, direct media, or local audio/video files)
        #[arg(value_name = "URL_OR_FILE")]
        url: String,

        /// Output file path (prints to console if not specified)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: OutputFormat,

        /// Language code for transcription (auto-detect if not specified)
        #[arg(short, long, value_name = "LANG")]
        language: Option<String>,

        /// Save the extracted audio file
        #[arg(long)]
        save_audio: bool,

        /// Enable speaker identification (shows who spoke when)
        #[arg(long)]
        speaker_labels: bool,

        /// Maximum number of speakers to identify (2-10, default: auto-detect)
        #[arg(long, value_name = "COUNT")]
        max_speakers: Option<u8>,

        /// Include timestamps in text output (srt/vtt formats always include timestamps)
        #[arg(long)]
        timestamps: bool,

        /// Use detailed timestamps with milliseconds (implies --timestamps)
        #[arg(long)]
        detailed_timestamps: bool,

        /// Maximum segment length in seconds (default: 10, helps create more frequent timestamps)
        #[arg(long, default_value = "10")]
        max_segment_length: f64,
    },

    /// Configure AWS credentials and settings
    Config {
        /// Show current configuration
        #[arg(short, long)]
        show: bool,
    },

    /// List supported platforms
    Platforms,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Plain text
    Text,
    /// JSON with timestamps
    Json,
    /// SRT subtitle format
    Srt,
    /// WebVTT format
    Vtt,
    /// CSV format
    Csv,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Srt => write!(f, "srt"),
            OutputFormat::Vtt => write!(f, "vtt"),
            OutputFormat::Csv => write!(f, "csv"),
        }
    }
} 