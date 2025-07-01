use anyhow::Result;
use std::path::Path;
use url::Url;

/// Validate a URL and return normalized version
pub fn validate_and_normalize_url(url: &str) -> Result<String> {
    let parsed = Url::parse(url)
        .map_err(|_| anyhow::anyhow!("Invalid URL format: {}", url))?;
    
    if !matches!(parsed.scheme(), "http" | "https") {
        anyhow::bail!("URL must use HTTP or HTTPS protocol");
    }
    
    Ok(parsed.to_string())
}

/// Format file size in human-readable format
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let bytes_f = bytes as f64;
    let unit_index = (bytes_f.log10() / THRESHOLD.log10()).floor() as usize;
    let unit_index = unit_index.min(UNITS.len() - 1);
    
    let size = bytes_f / THRESHOLD.powi(unit_index as i32);
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format duration in human-readable format
pub fn format_duration(seconds: f64) -> String {
    let total_seconds = seconds as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;
    
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Sanitize filename for safe filesystem usage
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| {
            match c {
                // Keep alphanumeric characters, spaces, hyphens, underscores, and dots
                c if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' => c,
                // Replace everything else with underscore
                _ => '_',
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Check if a file exists and is readable
pub fn check_file_accessible(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("File does not exist: {}", path.display());
    }
    
    if !path.is_file() {
        anyhow::bail!("Path is not a file: {}", path.display());
    }
    
    // Try to read metadata to check permissions
    std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("Cannot access file {}: {}", path.display(), e))?;
    
    Ok(())
}

/// Generate a unique filename with timestamp
pub fn generate_unique_filename(base_name: &str, extension: &str) -> String {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let random_suffix = uuid::Uuid::new_v4().to_string()[..8].to_string();
    
    format!("{}_{}_{}_{}.{}", 
        "transcriptor",
        sanitize_filename(base_name),
        timestamp,
        random_suffix,
        extension
    )
}

/// Parse language code and return normalized version
pub fn normalize_language_code(lang: &str) -> String {
    // Common language code mappings
    let normalized = match lang.to_lowercase().as_str() {
        "en" | "english" => "en-US",
        "es" | "spanish" => "es-ES", 
        "fr" | "french" => "fr-FR",
        "de" | "german" => "de-DE",
        "it" | "italian" => "it-IT",
        "pt" | "portuguese" => "pt-BR",
        "ja" | "japanese" => "ja-JP",
        "ko" | "korean" => "ko-KR",
        "zh" | "chinese" => "zh-CN",
        "ar" | "arabic" => "ar-SA",
        "hi" | "hindi" => "hi-IN",
        "ru" | "russian" => "ru-RU",
        _ => lang, // Return as-is if no mapping found
    };
    
    normalized.to_string()
}

/// Extract domain from URL for display purposes
pub fn extract_domain(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()?
        .host_str()
        .map(|host| {
            // Remove 'www.' prefix if present
            if host.starts_with("www.") {
                host[4..].to_string()
            } else {
                host.to_string()
            }
        })
}

/// Check if the current environment has required tools
pub async fn check_dependencies() -> Vec<String> {
    let mut missing = Vec::new();
    
    // Check for yt-dlp
    if !check_command_available("yt-dlp").await {
        missing.push("yt-dlp - required for YouTube and Twitter extraction".to_string());
    }
    
    // Check for ffmpeg (optional but recommended)
    if !check_command_available("ffmpeg").await {
        missing.push("ffmpeg - recommended for audio processing".to_string());
    }
    
    missing
}

/// Check if a command is available in PATH
async fn check_command_available(command: &str) -> bool {
    use tokio::process::Command;
    
    Command::new(command)
        .arg("--version")
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1048576), "1.0 MB");
    }
    
    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30.0), "30s");
        assert_eq!(format_duration(90.0), "1m 30s");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
    }
    
    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World!"), "Hello World_");
        assert_eq!(sanitize_filename("test/file?name"), "test_file_name");
        assert_eq!(sanitize_filename("  spaced  "), "spaced");
    }
    
    #[test]
    fn test_normalize_language_code() {
        assert_eq!(normalize_language_code("en"), "en-US");
        assert_eq!(normalize_language_code("English"), "en-US");
        assert_eq!(normalize_language_code("es"), "es-ES");
        assert_eq!(normalize_language_code("zh-TW"), "zh-TW"); // Pass through
    }
    
    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.youtube.com/watch?v=123"), Some("youtube.com".to_string()));
        assert_eq!(extract_domain("https://twitter.com/user/status/123"), Some("twitter.com".to_string()));
        assert_eq!(extract_domain("invalid-url"), None);
    }
    
    #[test]
    fn test_validate_and_normalize_url() {
        assert!(validate_and_normalize_url("https://example.com").is_ok());
        assert!(validate_and_normalize_url("http://example.com").is_ok());
        assert!(validate_and_normalize_url("ftp://example.com").is_err());
        assert!(validate_and_normalize_url("not-a-url").is_err());
    }
} 