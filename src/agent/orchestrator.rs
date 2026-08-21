use crate::agent::emoji::strip_emojis;
use crate::agent::form_machine::FormMachine;
use crate::agent::memory::ConversationMemory;
use crate::agent::prompt::{build_manager_system_prompt, build_worker_system_prompt};
use crate::config::Config;
use crate::models::{FormStep, IncomingMessage};
use crate::rag::vector_store::VectorStore;
use crate::voice::VoiceEngine;
use crate::whatsapp::WhatsAppClient;
use reqwest::Client;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

pub struct AgentOrchestrator {
    config: Config,
    whatsapp_client: WhatsAppClient,
    vector_store: Arc<VectorStore>,
    form_machine: Arc<FormMachine>,
    voice_engine: Arc<VoiceEngine>,
    memory: Arc<ConversationMemory>,
    http_client: Client,
}

impl AgentOrchestrator {
    pub fn new(
        config: Config,
        whatsapp_client: WhatsAppClient,
        vector_store: Arc<VectorStore>,
        form_machine: Arc<FormMachine>,
        voice_engine: Arc<VoiceEngine>,
        memory: Arc<ConversationMemory>,
    ) -> Self {
        Self {
            config,
            whatsapp_client,
            vector_store,
            form_machine,
            voice_engine,
            memory,
            http_client: Client::new(),
        }
    }

    /// Process incoming WhatsApp message end-to-end with persistent memory and RAG
    pub async fn process_incoming_message(&self, msg: IncomingMessage) -> anyhow::Result<()> {
        let sender = Config::normalize_phone(&msg.from);
        info!("Processing incoming message from [{}] type={}", sender, msg.msg_type);

        let is_manager = self.config.is_manager(&sender);
        let mut user_text = String::new();
        let mut wants_voice_reply = false;

        if msg.msg_type == "audio" {
            wants_voice_reply = true;
            if let Some(audio) = msg.audio {
                let temp_ogg = PathBuf::from(format!("/tmp/rx_{}.ogg", uuid::Uuid::new_v4()));
                if let Ok(()) = self.whatsapp_client.download_media(&audio.id, &temp_ogg).await {
                    match self.voice_engine.audio_to_text(&temp_ogg).await {
                        Ok(transcription) => {
                            user_text = transcription;
                            info!("Audio transcribed from {}: {}", sender, user_text);
                        }
                        Err(e) => {
                            error!("Failed to transcribe voice note from {}: {}", sender, e);
                            user_text = "رسالة صوتية".to_string();
                        }
                    }
                    let _ = tokio::fs::remove_file(&temp_ogg).await;
                }
            }
        } else if msg.msg_type == "text" {
            if let Some(text_obj) = msg.text {
                user_text = text_obj.body;
            }
        }

        let user_text = user_text.trim().to_string();
        if user_text.is_empty() {
            return Ok(());
        }

        // Check if user requested voice note reply
        if user_text.contains("صوت") || user_text.contains("ريكورد") || user_text.contains("صوتي") {
            wants_voice_reply = true;
        }

        // 1. Learn any user introduction from incoming text
        self.memory.learn_from_text(&sender, &user_text);

        // 2. Record incoming message into conversation memory
        self.memory.add_message(&sender, "user", &user_text);

        // 3. Route to manager or worker handler
        if is_manager {
            self.handle_manager_request(&sender, &user_text, wants_voice_reply).await?;
        } else {
            self.handle_worker_request(&sender, &user_text, wants_voice_reply).await?;
        }

        Ok(())
    }

    /// Handle Manager / Admin Sudo Mode requests
    async fn handle_manager_request(&self, sender: &str, query: &str, voice_reply: bool) -> anyhow::Result<()> {
        info!("Handling Manager Sudo Mode query from {}", sender);

        // Fetch context from Turbovec Vector Store
        let rag_context = self.vector_store.get_rag_context(query).await;

        let system_prompt = build_manager_system_prompt(&self.config.manager_phone, &rag_context);
        let history = self.memory.get_history(sender, 12);

        let raw_llm_response = self.call_chat_llm(&system_prompt, &history, query).await?;

        // STRICT ZERO-EMOJI ENFORCEMENT FOR MANAGER
        let sanitized_response = strip_emojis(&raw_llm_response);

        // Save assistant reply to memory
        self.memory.add_message(sender, "assistant", &sanitized_response);

        println!("\n==========================================");
        println!("  [MANAGER RESPONSE - ZERO EMOJIS]");
        println!("==========================================");
        println!("{}\n==========================================", sanitized_response);

        if voice_reply {
            let temp_ogg = PathBuf::from(format!("/tmp/reply_{}.ogg", uuid::Uuid::new_v4()));
            match self.voice_engine.text_to_audio(&sanitized_response, &temp_ogg).await {
                Ok(()) => {
                    let _ = self.whatsapp_client.send_voice_note(sender, &temp_ogg).await;
                    let _ = tokio::fs::remove_file(&temp_ogg).await;
                }
                Err(e) => {
                    error!("Voice synthesis failed, sending text as fallback: {}", e);
                    let _ = self.whatsapp_client.send_text_message(sender, &sanitized_response).await;
                }
            }
        } else {
            let _ = self.whatsapp_client.send_text_message(sender, &sanitized_response).await;
        }

        Ok(())
    }

    /// Handle Worker / Employee requests & Interactive Onboarding Forms
    async fn handle_worker_request(&self, sender: &str, text: &str, voice_reply: bool) -> anyhow::Result<()> {
        let current_state = self.form_machine.get_state(sender);

        let is_form_in_progress = current_state.step != FormStep::NotStarted && current_state.step != FormStep::Completed;
        let is_form_trigger = text.contains("تقديم") || text.contains("استمارة تسجيل") || text.contains("سجل بياناتي");

        let reply_text = if is_form_in_progress || is_form_trigger {
            let (form_reply, opt_profile) = self.form_machine.process_step(sender, text);
            if let Some(profile) = opt_profile {
                // Update persistent memory profile
                self.memory.update_user_info(
                    sender,
                    Some(profile.full_name.clone()),
                    Some(profile.job_role.clone()),
                    Some(profile.national_id.clone()),
                    Some(profile.daily_rate.clone()),
                    Some(profile.work_location.clone()),
                    Some(profile.notes.clone()),
                );

                // Index completed worker profile into RAG Vector Store
                let profile_str = format!(
                    "بيانات عامل مسجل: الاسم: {} | الوظيفة: {} | الهاتف: {} | القومي: {} | اليومية: {} جنيه | الموقع: {} | ملاحظات: {}",
                    profile.full_name, profile.job_role, sender, profile.national_id, profile.daily_rate, profile.work_location, profile.notes
                );
                self.vector_store
                    .add_document("worker_profile", sender, &profile.full_name, &profile_str)
                    .await;
            }
            form_reply
        } else {
            // Retrieve contextual RAG memory
            let rag_context = self.vector_store.get_rag_context(text).await;
            let user_summary = self.memory.get_user_summary(sender);

            let system_prompt = build_worker_system_prompt(
                sender,
                &user_summary,
                &self.config.manager_phone,
                &rag_context,
            );

            let history = self.memory.get_history(sender, 12);
            let response = self.call_chat_llm(&system_prompt, &history, text).await?;

            // Automatically record non-trivial user messages as daily logs / knowledge in VectorStore
            if text.len() > 6 && !text.starts_with("hi") && !text.starts_with("مرحب") && !text.starts_with("شكرا") {
                let user_name = self.memory.get_or_create_profile(sender).name.unwrap_or_else(|| sender.to_string());
                let log_title = format!("يوميات/ملاحظة - {}", user_name);
                self.vector_store
                    .add_document("daily_report", sender, &log_title, text)
                    .await;
            }

            response
        };

        // Record assistant response in memory
        self.memory.add_message(sender, "assistant", &reply_text);

        println!("\n==========================================");
        println!("  [WORKER RESPONSE]");
        println!("==========================================");
        println!("{}\n==========================================", reply_text);

        if voice_reply {
            let temp_ogg = PathBuf::from(format!("/tmp/wreply_{}.ogg", uuid::Uuid::new_v4()));
            match self.voice_engine.text_to_audio(&reply_text, &temp_ogg).await {
                Ok(()) => {
                    let _ = self.whatsapp_client.send_voice_note(sender, &temp_ogg).await;
                    let _ = tokio::fs::remove_file(&temp_ogg).await;
                }
                Err(_) => {
                    let _ = self.whatsapp_client.send_text_message(sender, &reply_text).await;
                }
            }
        } else {
            let _ = self.whatsapp_client.send_text_message(sender, &reply_text).await;
        }

        Ok(())
    }

    /// Query Chat LLM (Groq Llama-3.3-70B with multi-turn history, fallback to Mistral)
    pub async fn call_chat_llm(
        &self,
        system_prompt: &str,
        history: &[crate::agent::memory::ChatMessage],
        current_user_message: &str,
    ) -> anyhow::Result<String> {
        let mut messages_payload = Vec::new();

        // 1. System Prompt
        messages_payload.push(json!({
            "role": "system",
            "content": system_prompt
        }));

        // 2. Multi-turn conversation history (excluding the very last duplicate message if present)
        for msg in history.iter().take(history.len().saturating_sub(1)) {
            messages_payload.push(json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        // 3. Current User Message
        messages_payload.push(json!({
            "role": "user",
            "content": current_user_message
        }));

        // 1. Try Groq Llama-3.3-70B first
        if !self.config.groq_api_key.is_empty() {
            let groq_payload = json!({
                "model": "llama-3.3-70b-versatile",
                "temperature": 0.2,
                "max_tokens": 1024,
                "messages": messages_payload
            });

            if let Ok(resp) = self
                .http_client
                .post("https://api.groq.com/openai/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", self.config.groq_api_key))
                .header("Content-Type", "application/json")
                .json(&groq_payload)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(json_val) = resp.json::<serde_json::Value>().await {
                        if let Some(content) = json_val["choices"][0]["message"]["content"].as_str() {
                            return Ok(content.to_string());
                        }
                    }
                }
            }
        }

        // 2. Fallback to Mistral AI
        if !self.config.mistral_api_key.is_empty() {
            let payload = json!({
                "model": "mistral-small-latest",
                "temperature": 0.2,
                "max_tokens": 1024,
                "messages": messages_payload
            });

            let resp = self
                .http_client
                .post("https://api.mistral.ai/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", self.config.mistral_api_key))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await?;

            if resp.status().is_success() {
                if let Ok(json_val) = resp.json::<serde_json::Value>().await {
                    if let Some(content) = json_val["choices"][0]["message"]["content"].as_str() {
                        return Ok(content.to_string());
                    }
                }
            }
        }

        Ok("عذراً، حدث خطأ مؤقت في الاتصال بالذكاء الاصطناعي.".to_string())
    }
}
