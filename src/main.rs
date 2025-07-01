use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;
mod config;
mod extractors;
mod output;
mod transcribe;
mod utils;

use cli::{Cli, Commands};
use config::Config;
use transcribe::TranscriptionPipeline;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "universal_transcriptor=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    let config = Config::load().await?;

    match cli.command {
        Commands::Transcribe {
            url,
            output,
            format,
            language,
            save_audio,
            speaker_labels,
            max_speakers,
            no_timestamps,
            detailed_timestamps,
            max_segment_length,
        } => {
            let pipeline = TranscriptionPipeline::new(config).await?;
            
            tracing::info!("Starting transcription for URL: {}", url);
            
            let result = pipeline
                .transcribe_from_url(&url, language.as_deref(), speaker_labels, max_speakers, max_segment_length)
                .await?;

            // Handle output
            match output {
                Some(path) => {
                    output::save_to_file(&result, &path, &format, !no_timestamps, detailed_timestamps).await?;
                    println!("Transcription saved to: {}", path.display());
                }
                None => {
                    output::print_to_console(&result, &format, !no_timestamps, detailed_timestamps)?;
                }
            }

            // Save audio if requested
            if save_audio {
                if let Some(audio_path) = result.audio_path {
                    println!("Audio saved to: {}", audio_path.display());
                }
            }
        }
        Commands::Config { show } => {
            if show {
                config.display();
            } else {
                config.interactive_setup().await?;
            }
        }
        Commands::Platforms => {
            println!("Supported platforms:");
            println!("  • YouTube (youtube.com, youtu.be)");
            println!("  • Twitter/X (twitter.com, x.com)");
            println!("  • Direct audio/video URLs");
            println!("  • More platforms coming soon!");
        }
    }

    Ok(())
} 