use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" | "assistant" | "system"
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserMemoryProfile {
    pub phone: String,
    pub name: Option<String>,
    pub job_role: Option<String>,
    pub national_id: Option<String>,
    pub daily_rate: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub last_active: String,
    pub messages: Vec<ChatMessage>,
}

pub struct ConversationMemory {
    profiles: Arc<DashMap<String, UserMemoryProfile>>,
    storage_path: String,
}

impl ConversationMemory {
    pub fn new(storage_path: &str) -> Self {
        let memory = Self {
            profiles: Arc::new(DashMap::new()),
            storage_path: storage_path.to_string(),
        };

        memory.load_from_disk();
        memory
    }

    /// Load existing user memory profiles from disk if available
    fn load_from_disk(&self) {
        let path = Path::new(&self.storage_path);
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(path) {
                if let Ok(loaded_profiles) = serde_json::from_str::<Vec<UserMemoryProfile>>(&data) {
                    for profile in loaded_profiles {
                        self.profiles.insert(profile.phone.clone(), profile);
                    }
                    info!("Loaded {} user memory profiles from disk.", self.profiles.len());
                }
            }
        }
    }

    /// Persist all memory profiles to disk
    pub fn save_to_disk(&self) {
        let all_profiles: Vec<UserMemoryProfile> = self
            .profiles
            .iter()
            .map(|r| r.value().clone())
            .collect();

        if let Ok(json_str) = serde_json::to_string_pretty(&all_profiles) {
            let _ = std::fs::write(&self.storage_path, json_str);
        }
    }

    pub fn get_or_create_profile(&self, phone: &str) -> UserMemoryProfile {
        self.profiles
            .entry(phone.to_string())
            .or_insert_with(|| UserMemoryProfile {
                phone: phone.to_string(),
                last_active: Utc::now().to_rfc3339(),
                ..Default::default()
            })
            .clone()
    }

    pub fn add_message(&self, phone: &str, role: &str, content: &str) {
        let mut profile = self.get_or_create_profile(phone);
        profile.last_active = Utc::now().to_rfc3339();

        profile.messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });

        // Keep last 20 messages for prompt efficiency
        if profile.messages.len() > 20 {
            profile.messages.drain(0..profile.messages.len() - 20);
        }

        self.profiles.insert(phone.to_string(), profile);
        self.save_to_disk();
    }

    pub fn get_history(&self, phone: &str, limit: usize) -> Vec<ChatMessage> {
        let profile = self.get_or_create_profile(phone);
        let len = profile.messages.len();
        if len <= limit {
            profile.messages
        } else {
            profile.messages[len - limit..].to_vec()
        }
    }

    pub fn update_user_info(
        &self,
        phone: &str,
        name: Option<String>,
        job_role: Option<String>,
        national_id: Option<String>,
        daily_rate: Option<String>,
        location: Option<String>,
        notes: Option<String>,
    ) {
        let mut profile = self.get_or_create_profile(phone);
        if let Some(n) = name {
            profile.name = Some(n);
        }
        if let Some(j) = job_role {
            profile.job_role = Some(j);
        }
        if let Some(nid) = national_id {
            profile.national_id = Some(nid);
        }
        if let Some(d) = daily_rate {
            profile.daily_rate = Some(d);
        }
        if let Some(loc) = location {
            profile.location = Some(loc);
        }
        if let Some(not) = notes {
            profile.notes = Some(not);
        }
        profile.last_active = Utc::now().to_rfc3339();
        self.profiles.insert(phone.to_string(), profile);
        self.save_to_disk();
    }

    /// Try to extract and learn user's name if they explicitly introduce themselves
    pub fn learn_from_text(&self, phone: &str, text: &str) {
        let cleaned = text.trim();
        if cleaned.starts_with("اسمي ") || cleaned.starts_with("إسمي ") || cleaned.starts_with("انا اسمي ") || cleaned.starts_with("أنا اسمي ") || cleaned.starts_with("أنا إسمي ") {
            let name_part = cleaned
                .replace("اسمي ", "")
                .replace("إسمي ", "")
                .replace("انا اسمي ", "")
                .replace("أنا اسمي ", "")
                .replace("أنا إسمي ", "")
                .trim()
                .to_string();

            // Extract first 1-4 words as name
            let words: Vec<&str> = name_part.split_whitespace().collect();
            if !words.is_empty() && words.len() <= 5 {
                let extracted_name = words.join(" ");
                info!("Learned user name for [{}]: {}", phone, extracted_name);
                self.update_user_info(phone, Some(extracted_name), None, None, None, None, None);
            }
        }
    }

    /// Build a concise summary of what we know about this user
    pub fn get_user_summary(&self, phone: &str) -> String {
        let profile = self.get_or_create_profile(phone);
        let mut parts = Vec::new();

        if let Some(ref name) = profile.name {
            parts.push(format!("الاسم: {}", name));
        }
        if let Some(ref job) = profile.job_role {
            parts.push(format!("الوظيفة: {}", job));
        }
        if let Some(ref loc) = profile.location {
            parts.push(format!("موقع العمل: {}", loc));
        }
        if let Some(ref rate) = profile.daily_rate {
            parts.push(format!("اليومية: {} جنيه", rate));
        }
        if let Some(ref nid) = profile.national_id {
            parts.push(format!("الرقم القومي/الهوية: {}", nid));
        }
        if let Some(ref not) = profile.notes {
            if not != "لا يوجد" && not != "لا" {
                parts.push(format!("ملاحظات: {}", not));
            }
        }

        if parts.is_empty() {
            "مستخدم غير مسجل بياناته بعد.".to_string()
        } else {
            parts.join(" | ")
        }
    }
}
