use anyhow::Result;
use std::path::Path;

use crate::cli::OutputFormat;
use crate::transcribe::TranscriptionResult;

pub mod formatters;

pub use formatters::*;

/// Save transcription result to file
pub async fn save_to_file(
    result: &TranscriptionResult,
    path: &Path,
    format: &OutputFormat,
    include_timestamps: bool,
    detailed_timestamps: bool,
) -> Result<()> {
    let content = match format {
        OutputFormat::Text => format_as_text(result, include_timestamps, detailed_timestamps),
        OutputFormat::Json => format_as_json(result)?,
        OutputFormat::Srt => format_as_srt(result, detailed_timestamps),
        OutputFormat::Vtt => format_as_vtt(result, detailed_timestamps),
        OutputFormat::Csv => format_as_csv(result)?,
    };
    
    fs_err::write(path, content)?;
    Ok(())
}

/// Print transcription result to console
pub fn print_to_console(
    result: &TranscriptionResult, 
    format: &OutputFormat,
    include_timestamps: bool,
    detailed_timestamps: bool,
) -> Result<()> {
    let content = match format {
        OutputFormat::Text => format_as_text(result, include_timestamps, detailed_timestamps),
        OutputFormat::Json => format_as_json(result)?,
        OutputFormat::Srt => format_as_srt(result, detailed_timestamps),
        OutputFormat::Vtt => format_as_vtt(result, detailed_timestamps),
        OutputFormat::Csv => format_as_csv(result)?,
    };
    
    println!("{}", content);
    Ok(())
} 