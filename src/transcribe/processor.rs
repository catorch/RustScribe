use anyhow::{Context, Result};
use aws_sdk_transcribe::Client as TranscribeClient;
use aws_sdk_transcribe::types::{TranscriptionJob, TranscriptionJobStatus};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

use super::{TranscriptSegment, TranscriptionMetadata};
use crate::output::formatters::WordTimestamp;

/// Processed transcription result from AWS
#[derive(Debug, Clone)]
pub struct ProcessedTranscription {
    pub transcript: String,
    pub segments: Vec<TranscriptSegment>,
    pub metadata: TranscriptionMetadata,
    pub words: Option<Vec<WordTimestamp>>,
}

/// AWS Transcribe transcript format
#[derive(Debug, Deserialize)]
struct AwsTranscript {
    #[serde(rename = "jobName")]
    job_name: String,
    #[serde(rename = "accountId")]
    account_id: String,
    results: TranscriptResults,
    status: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptResults {
    transcripts: Vec<TranscriptText>,
    items: Vec<TranscriptItem>,
    #[serde(rename = "speaker_labels")]
    speaker_labels: Option<SpeakerLabels>,
}

#[derive(Debug, Deserialize)]
struct TranscriptText {
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptItem {
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
    alternatives: Vec<Alternative>,
    speaker_label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    confidence: Option<String>,
    content: String,
}

#[derive(Debug, Deserialize)]
struct SpeakerLabels {
    speakers: u32,
    segments: Vec<SpeakerSegment>,
}

#[derive(Debug, Deserialize)]
struct SpeakerSegment {
    start_time: String,
    end_time: String,
    speaker_label: String,
    items: Vec<SpeakerItem>,
}

#[derive(Debug, Deserialize)]
struct SpeakerItem {
    start_time: String,
    end_time: String,
    speaker_label: String,
}

/// Transcription job processor
pub struct TranscriptionProcessor {
    client: TranscribeClient,
    job_id: String,
    max_segment_length: f64,
}

impl TranscriptionProcessor {
    pub fn new(client: TranscribeClient, job_id: String, max_segment_length: f64) -> Self {
        Self { client, job_id, max_segment_length }
    }
    
    /// Wait for transcription job completion with progress tracking
    pub async fn wait_for_completion(&self) -> Result<ProcessedTranscription> {
        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap()
        );
        progress.set_message("Starting transcription job...");
        
        let start_time = std::time::Instant::now();
        let mut check_count = 0;
        
        loop {
            check_count += 1;
            
            // Get job status
            let job = self.get_transcription_job().await?;
            
            match job.transcription_job_status() {
                Some(TranscriptionJobStatus::InProgress) => {
                    progress.set_message(format!(
                        "Transcribing... ({}s elapsed, check #{})",
                        start_time.elapsed().as_secs(),
                        check_count
                    ));
                    
                    // Wait before next check (exponential backoff up to 30 seconds)
                    let wait_time = std::cmp::min(5 + (check_count - 1) * 2, 30);
                    sleep(Duration::from_secs(wait_time)).await;
                }
                Some(TranscriptionJobStatus::Completed) => {
                    progress.finish_with_message("Transcription completed!");
                    break;
                }
                Some(TranscriptionJobStatus::Failed) => {
                    progress.finish_with_message("Transcription failed");
                    
                    let failure_reason = job.failure_reason()
                        .unwrap_or("Unknown error");
                    anyhow::bail!("Transcription job failed: {}", failure_reason);
                }
                _ => {
                    progress.finish_with_message("Transcription status unknown");
                    anyhow::bail!("Unexpected transcription job status");
                }
            }
        }
        
        // Get and process the results
        let job = self.get_transcription_job().await?;
        self.process_transcription_result(job, start_time.elapsed()).await
    }
    
    /// Get transcription job details
    async fn get_transcription_job(&self) -> Result<TranscriptionJob> {
        let response = self.client
            .get_transcription_job()
            .transcription_job_name(&self.job_id)
            .send()
            .await
            .context("Failed to get transcription job status")?;
            
        response.transcription_job()
            .ok_or_else(|| anyhow::anyhow!("Transcription job not found"))
            .map(|job| job.clone())
    }
    
    /// Process completed transcription result
    async fn process_transcription_result(
        &self,
        job: TranscriptionJob,
        processing_duration: std::time::Duration,
    ) -> Result<ProcessedTranscription> {
        // Get transcript URI
        let transcript_uri = job.transcript()
            .and_then(|t| t.transcript_file_uri())
            .ok_or_else(|| anyhow::anyhow!("No transcript URI found"))?;
            
        // Download transcript JSON
        let transcript_json = self.download_transcript(transcript_uri).await?;
        
        // Parse transcript
        let aws_transcript: AwsTranscript = serde_json::from_str(&transcript_json)
            .context("Failed to parse transcript JSON")?;
            
        // Extract main transcript text
        let transcript = aws_transcript.results.transcripts
            .first()
            .map(|t| t.transcript.clone())
            .unwrap_or_default();
            
        // Process segments with timestamps
        let (segments, words) = self.process_segments(&aws_transcript.results)?;
        
        // Create metadata
        let metadata = TranscriptionMetadata {
            job_id: self.job_id.clone(),
            language: job.language_code()
                .map(|lc| lc.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            processing_duration: Some(processing_duration.as_secs_f64()),
            audio_duration: segments.last().map(|s| s.end_time),
            confidence: self.calculate_average_confidence(&segments),
            completed_at: chrono::Utc::now(),
        };
        
        Ok(ProcessedTranscription {
            transcript,
            segments,
            metadata,
            words: Some(words),
        })
    }
    
    /// Download transcript from S3
    async fn download_transcript(&self, uri: &str) -> Result<String> {
        let response = reqwest::get(uri).await
            .context("Failed to download transcript")?;
            
        if !response.status().is_success() {
            anyhow::bail!("Failed to download transcript: HTTP {}", response.status());
        }
        
        let content = response.text().await
            .context("Failed to read transcript content")?;
            
        Ok(content)
    }
    
    /// Process transcript items into segments and extract word-level timestamps
    fn process_segments(&self, results: &TranscriptResults) -> Result<(Vec<TranscriptSegment>, Vec<WordTimestamp>)> {
        let mut segments = Vec::new();
        let mut words = Vec::new();
        
        // First pass: extract all word-level timestamps
        for item in &results.items {
            if item.item_type == "pronunciation" {
                if let (Some(start_str), Some(end_str)) = (&item.start_time, &item.end_time) {
                    if let (Ok(start_time), Ok(end_time)) = (start_str.parse::<f64>(), end_str.parse::<f64>()) {
                        if let Some(alt) = item.alternatives.first() {
                            words.push(WordTimestamp {
                                word: alt.content.clone(),
                                start_time,
                                end_time,
                                confidence: alt.confidence.as_ref().and_then(|c| c.parse::<f64>().ok()),
                                speaker_id: item.speaker_label.clone(),
                            });
                        }
                    }
                }
            }
        }
        
        // Second pass: group words into segments (existing logic)
        let mut current_segment_text = String::new();
        let mut current_start_time: Option<f64> = None;
        let mut current_end_time: Option<f64> = None;
        let mut confidences = Vec::new();
        let mut current_speaker: Option<String> = None;
        
        for item in &results.items {
            if item.item_type == "pronunciation" {
                let start_time = item.start_time.as_ref()
                    .and_then(|s| s.parse::<f64>().ok());
                let end_time = item.end_time.as_ref()
                    .and_then(|s| s.parse::<f64>().ok());
                    
                let content = item.alternatives.first()
                    .map(|alt| alt.content.clone())
                    .unwrap_or_default();
                    
                let confidence = item.alternatives.first()
                    .and_then(|alt| alt.confidence.as_ref())
                    .and_then(|c| c.parse::<f64>().ok());
                    
                // Start new segment if speaker changes, significant gap, or segment is getting too long
                let speaker_changed = current_speaker.as_ref() != item.speaker_label.as_ref();
                let time_gap = start_time.zip(current_end_time)
                    .map(|(start, end)| start - end > 1.0)
                    .unwrap_or(false);
                    
                let segment_too_long = current_start_time.zip(start_time)
                    .map(|(seg_start, current)| current - seg_start > self.max_segment_length)
                    .unwrap_or(false);
                    
                let natural_break = content.ends_with('.') || content.ends_with('!') || content.ends_with('?');
                    
                let min_natural_break_length = self.max_segment_length / 2.0;
                let should_split = speaker_changed || time_gap || segment_too_long || 
                    (natural_break && current_start_time.zip(start_time).map(|(seg_start, current)| current - seg_start > min_natural_break_length).unwrap_or(false)) ||
                    current_segment_text.is_empty();
                    
                if should_split {
                    // Save previous segment if it exists
                    if !current_segment_text.is_empty() {
                        if let (Some(start), Some(end)) = (current_start_time, current_end_time) {
                            segments.push(TranscriptSegment {
                                start_time: start,
                                end_time: end,
                                text: current_segment_text.trim().to_string(),
                                confidence: self.average_confidence(&confidences),
                                speaker_id: current_speaker.clone(),
                            });
                        }
                    }
                    
                    // Start new segment
                    current_segment_text = content.clone();
                    current_start_time = start_time;
                    current_end_time = end_time;
                    confidences = confidence.into_iter().collect();
                    current_speaker = item.speaker_label.clone();
                } else {
                    // Continue current segment
                    if !current_segment_text.is_empty() {
                        current_segment_text.push(' ');
                    }
                    current_segment_text.push_str(&content);
                    current_end_time = end_time.or(current_end_time);
                    
                    if let Some(conf) = confidence {
                        confidences.push(conf);
                    }
                }
            } else if item.item_type == "punctuation" {
                // Add punctuation to current segment
                if let Some(alt) = item.alternatives.first() {
                    current_segment_text.push_str(&alt.content);
                }
            }
        }
        
        // Add final segment
        if !current_segment_text.is_empty() {
            if let (Some(start), Some(end)) = (current_start_time, current_end_time) {
                segments.push(TranscriptSegment {
                    start_time: start,
                    end_time: end,
                    text: current_segment_text.trim().to_string(),
                    confidence: self.average_confidence(&confidences),
                    speaker_id: current_speaker,
                });
            }
        }
        
        Ok((segments, words))
    }
    
    /// Calculate average confidence from a list
    fn average_confidence(&self, confidences: &[f64]) -> Option<f64> {
        if confidences.is_empty() {
            None
        } else {
            Some(confidences.iter().sum::<f64>() / confidences.len() as f64)
        }
    }
    
    /// Calculate overall average confidence
    fn calculate_average_confidence(&self, segments: &[TranscriptSegment]) -> Option<f64> {
        let confidences: Vec<f64> = segments
            .iter()
            .filter_map(|s| s.confidence)
            .collect();
            
        self.average_confidence(&confidences)
    }
} 