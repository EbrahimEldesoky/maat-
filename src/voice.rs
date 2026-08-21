use reqwest::Client;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use tracing::{error, info};

pub struct VoiceEngine {
    mistral_api_key: String,
    http_client: Client,
}

impl VoiceEngine {
    pub fn new(mistral_api_key: String) -> Self {
        Self {
            mistral_api_key,
            http_client: Client::new(),
        }
    }

    /// Speech-to-Text (STT): Transcribes Egyptian audio to text via Mistral / Whisper API or ffmpeg fallback
    pub async fn audio_to_text(&self, audio_path: &Path) -> anyhow::Result<String> {
        if !audio_path.exists() {
            anyhow::bail!("Audio file does not exist: {:?}", audio_path);
        }

        info!("Transcribing audio file: {:?}", audio_path);

        // Convert audio to mp3 for maximum API compatibility using ffmpeg
        let mp3_temp = NamedTempFile::new()?;
        let mp3_path = mp3_temp.path().with_extension("mp3");

        let ffmpeg_status = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(audio_path)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg(&mp3_path)
            .output();

        let target_file = match ffmpeg_status {
            Ok(output) if output.status.success() => mp3_path.clone(),
            _ => audio_path.to_path_buf(),
        };

        if !self.mistral_api_key.is_empty() {
            let file_bytes = tokio::fs::read(&target_file).await?;
            let part = reqwest::multipart::Part::bytes(file_bytes)
                .file_name("audio.mp3")
                .mime_str("audio/mpeg")?;

            let form = reqwest::multipart::Form::new()
                .text("model", "voxtral-mini-latest")
                .part("file", part);

            let resp = self
                .http_client
                .post("https://api.mistral.ai/v1/audio/transcriptions")
                .header("Authorization", format!("Bearer {}", self.mistral_api_key))
                .multipart(form)
                .send()
                .await;

            if let Ok(r) = resp {
                if r.status().is_success() {
                    let json_res: serde_json::Value = r.json().await?;
                    if let Some(text) = json_res["text"].as_str() {
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            info!("Successfully transcribed audio: {}", trimmed);
                            let _ = tokio::fs::remove_file(&mp3_path).await;
                            return Ok(trimmed);
                        }
                    }
                }
            }
        }

        let _ = tokio::fs::remove_file(&mp3_path).await;
        anyhow::bail!("Voice transcription could not be completed.")
    }

    /// Text-to-Speech (TTS): Converts response text to natural Egyptian Arabic voice note (Opus/OGG)
    pub async fn text_to_audio(&self, text: &str, output_ogg_path: &Path) -> anyhow::Result<()> {
        let clean_text = text.trim();
        if clean_text.is_empty() {
            anyhow::bail!("Text for TTS cannot be empty.");
        }

        info!("Generating Egyptian Arabic voice for text: {}", clean_text);

        let mp3_temp = PathBuf::from(format!("/tmp/tts_{}.mp3", uuid::Uuid::new_v4()));

        // Run edge-tts with Egyptian Arabic voice ar-EG-SalmaNeural
        let edge_tts_cmd = Command::new(".venv/bin/edge-tts")
            .arg("--voice")
            .arg("ar-EG-SalmaNeural")
            .arg("--text")
            .arg(clean_text)
            .arg("--write-media")
            .arg(&mp3_temp)
            .output();

        let tts_success = match edge_tts_cmd {
            Ok(out) => out.status.success(),
            Err(_) => {
                // Fallback to system edge-tts if venv path differs
                let fallback = Command::new("edge-tts")
                    .arg("--voice")
                    .arg("ar-EG-SalmaNeural")
                    .arg("--text")
                    .arg(clean_text)
                    .arg("--write-media")
                    .arg(&mp3_temp)
                    .output();
                fallback.map(|o| o.status.success()).unwrap_or(false)
            }
        };

        if !tts_success || !mp3_temp.exists() {
            anyhow::bail!("edge-tts failed to synthesize speech audio.");
        }

        // Convert MP3 to OGG Opus for native WhatsApp Voice Note compatibility
        let ffmpeg_cmd = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(&mp3_temp)
            .arg("-c:a")
            .arg("libopus")
            .arg("-b:a")
            .arg("32k")
            .arg("-vbr")
            .arg("on")
            .arg(output_ogg_path)
            .output();

        let _ = tokio::fs::remove_file(&mp3_temp).await;

        match ffmpeg_cmd {
            Ok(out) if out.status.success() && output_ogg_path.exists() => {
                info!("Successfully created OGG Opus voice note at {:?}", output_ogg_path);
                Ok(())
            }
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr);
                error!("FFmpeg audio conversion error: {}", err_msg);
                anyhow::bail!("FFmpeg conversion failed: {}", err_msg);
            }
            Err(e) => anyhow::bail!("FFmpeg command execution failed: {}", e),
        }
    }
}
