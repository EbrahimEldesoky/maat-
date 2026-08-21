use tracing::info;

#[derive(Debug, Clone)]
pub struct Config {
    // Evolution WhatsApp API
    pub evolution_api_url: String,
    pub evolution_api_key: String,
    pub evolution_instance_name: String,

    // LLM & Search
    pub groq_api_key: String,
    pub mistral_api_key: String,
    pub tavily_api_key: String,

    // Admin & Routing
    pub manager_phone: String,
    pub port: u16,
}

impl Config {
    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

        let evolution_api_url = std::env::var("EVOLUTION_API_URL")
            .unwrap_or_else(|_| "http://localhost:8085".to_string());
        let evolution_api_key = std::env::var("EVOLUTION_API_KEY")
            .unwrap_or_else(|_| "maat_evolution_secret_key_2026".to_string());
        let evolution_instance_name = std::env::var("EVOLUTION_INSTANCE_NAME")
            .unwrap_or_else(|_| "maat".to_string());

        let groq_api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
        let mistral_api_key = std::env::var("MISTRAL_API_KEY").unwrap_or_default();
        let tavily_api_key = std::env::var("TAVILY_API_KEY").unwrap_or_default();

        let manager_phone = std::env::var("MANAGER_PHONE")
            .unwrap_or_else(|_| "201224455366".to_string());
        let port: u16 = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        info!("Loaded maat configuration successfully.");
        info!("WhatsApp Gateway: Evolution API at {}", evolution_api_url);
        if !groq_api_key.is_empty() {
            info!("Groq Ultra-Fast LLM: Active");
        }
        if !mistral_api_key.is_empty() {
            info!("Mistral AI & Vector Embeddings: Active");
        }
        info!("Manager Phone: {}", manager_phone);

        Self {
            evolution_api_url,
            evolution_api_key,
            evolution_instance_name,
            groq_api_key,
            mistral_api_key,
            tavily_api_key,
            manager_phone,
            port,
        }
    }

    pub fn normalize_phone(phone: &str) -> String {
        phone.chars().filter(|c| c.is_ascii_digit()).collect()
    }

    pub fn is_manager(&self, phone: &str) -> bool {
        let norm_input = Self::normalize_phone(phone);
        let norm_mgr = Self::normalize_phone(&self.manager_phone);
        norm_input == norm_mgr
    }
}
