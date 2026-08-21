use crate::config::Config;
use reqwest::Client;
use serde_json::json;
use std::path::Path;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct WhatsAppClient {
    config: Config,
    client: Client,
}

impl WhatsAppClient {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Send text message with automatic retry on connection drops
    pub async fn send_text_message(&self, to: &str, text: &str) -> anyhow::Result<()> {
        let clean_to = Config::normalize_phone(to);
        let evo_url = format!(
            "{}/message/sendText/{}",
            self.config.evolution_api_url, self.config.evolution_instance_name
        );

        let evo_payload = json!({
            "number": clean_to,
            "text": text
        });

        let mut attempts = 0;
        let max_attempts = 3;

        while attempts < max_attempts {
            attempts += 1;

            let resp = self
                .client
                .post(&evo_url)
                .header("apikey", &self.config.evolution_api_key)
                .header("Content-Type", "application/json")
                .json(&evo_payload)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    info!("Sent WhatsApp message via Evolution API to {}", clean_to);
                    return Ok(());
                }
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    warn!(
                        "Evolution API send attempt {}/{} failed for {}: {} - {}",
                        attempts, max_attempts, clean_to, status, body
                    );

                    // If connection closed or busy, wait before retry
                    if attempts < max_attempts {
                        sleep(Duration::from_millis(800 * attempts as u64)).await;
                    } else {
                        error!("All {} attempts to send WhatsApp message to {} failed.", max_attempts, clean_to);
                        anyhow::bail!("WhatsApp send failed: {} - {}", status, body);
                    }
                }
                Err(e) => {
                    warn!(
                        "HTTP error on send attempt {}/{} for {}: {}",
                        attempts, max_attempts, clean_to, e
                    );
                    if attempts < max_attempts {
                        sleep(Duration::from_millis(800 * attempts as u64)).await;
                    } else {
                        anyhow::bail!("WhatsApp HTTP error: {}", e);
                    }
                }
            }
        }

        anyhow::bail!("Failed to send WhatsApp message after {} attempts", max_attempts)
    }

    /// Upload and send a voice note (audio message) via Evolution API
    pub async fn send_voice_note(&self, to: &str, ogg_path: &Path) -> anyhow::Result<()> {
        if !ogg_path.exists() {
            anyhow::bail!("Audio file does not exist at {:?}", ogg_path);
        }

        let clean_to = Config::normalize_phone(to);
        let evo_url = format!(
            "{}/message/sendWhatsAppAudio/{}",
            self.config.evolution_api_url, self.config.evolution_instance_name
        );

        let audio_bytes = tokio::fs::read(ogg_path).await?;
        let base64_audio = format!("data:audio/ogg;base64,{}", rbase64_encode(&audio_bytes));

        let evo_payload = json!({
            "number": clean_to,
            "audio": base64_audio
        });

        let resp = self
            .client
            .post(&evo_url)
            .header("apikey", &self.config.evolution_api_key)
            .header("Content-Type", "application/json")
            .json(&evo_payload)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                info!("Sent WhatsApp voice note to {}", clean_to);
                Ok(())
            }
            _ => {
                // If sending audio format fails, fallback gracefully to text message
                info!("Fallback voice synthesis to text message for {}", clean_to);
                self.send_text_message(to, "تم استلام رسالتك ومعالجتها بنجاح.").await
            }
        }
    }

    /// Download media file from Evolution API / Remote URL
    pub async fn download_media(&self, media_url: &str, save_path: &Path) -> anyhow::Result<()> {
        if media_url.starts_with("http://") || media_url.starts_with("https://") {
            let resp = self.client.get(media_url).send().await?;
            if resp.status().is_success() {
                let bytes = resp.bytes().await?;
                tokio::fs::write(save_path, bytes).await?;
                return Ok(());
            }
        }
        anyhow::bail!("Failed to download media from {}", media_url);
    }
}

fn rbase64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        let _ = write!(out, "{}", CHARSET[((n >> 18) & 63) as usize] as char);
        let _ = write!(out, "{}", CHARSET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            let _ = write!(out, "{}", CHARSET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            let _ = write!(out, "{}", CHARSET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
