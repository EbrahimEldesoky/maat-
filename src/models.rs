#![allow(dead_code)]
use serde::{Deserialize, Serialize};

/// WhatsApp Webhook Incoming Payload structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub object: Option<String>,
    pub entry: Option<Vec<WebhookEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEntry {
    pub id: Option<String>,
    pub changes: Option<Vec<WebhookChange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookChange {
    pub field: Option<String>,
    pub value: Option<WebhookValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookValue {
    pub messaging_product: Option<String>,
    pub metadata: Option<WebhookMetadata>,
    pub contacts: Option<Vec<WebhookContact>>,
    pub messages: Option<Vec<IncomingMessage>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookMetadata {
    pub display_phone_number: Option<String>,
    pub phone_number_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookContact {
    pub profile: Option<ProfileName>,
    pub wa_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileName {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub from: String,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub text: Option<MessageText>,
    pub audio: Option<MessageAudio>,
    pub interactive: Option<MessageInteractive>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageText {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAudio {
    pub id: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInteractive {
    #[serde(rename = "type")]
    pub interactive_type: Option<String>,
    pub button_reply: Option<ButtonReply>,
    pub list_reply: Option<ListReply>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonReply {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListReply {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
}

/// Dynamic Application / Worker Profile Data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkerProfile {
    pub phone_number: String,
    pub full_name: String,
    pub job_role: String,
    pub national_id: String,
    pub daily_rate: String,
    pub work_location: String,
    pub notes: String,
    pub registered_at: String,
}

/// Interactive Form State per User Phone Number
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FormStep {
    NotStarted,
    AskFullName,
    AskJobRole,
    AskNationalId,
    AskDailyRate,
    AskWorkLocation,
    AskNotes,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFormState {
    pub step: FormStep,
    pub profile: WorkerProfile,
}

/// RAG Vector Document Record stored in Turbovec/Memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: String,
    pub doc_type: String, // "worker_profile", "daily_report", "expense", "general_note"
    pub phone_number: String,
    pub title: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub timestamp: String,
}
