mod agent;
mod config;
mod models;
mod rag;
mod voice;
mod whatsapp;

use agent::form_machine::FormMachine;
use agent::memory::ConversationMemory;
use agent::orchestrator::AgentOrchestrator;
use config::Config;
use rag::vector_store::VectorStore;
use voice::VoiceEngine;
use whatsapp::WhatsAppClient;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub orchestrator: Arc<AgentOrchestrator>,
    pub vector_store: Arc<VectorStore>,
    pub memory: Arc<ConversationMemory>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    info!("Starting maat (ماعت) WhatsApp Personal Assistant Engine 2.0...");

    let config = Config::load();
    let whatsapp_client = WhatsAppClient::new(config.clone());
    let vector_store = Arc::new(VectorStore::new(config.mistral_api_key.clone()));
    let form_machine = Arc::new(FormMachine::new());
    let voice_engine = Arc::new(VoiceEngine::new(config.mistral_api_key.clone()));
    let memory = Arc::new(ConversationMemory::new("maat_memory.json"));

    let orchestrator = Arc::new(AgentOrchestrator::new(
        config.clone(),
        whatsapp_client,
        vector_store.clone(),
        form_machine,
        voice_engine,
        memory.clone(),
    ));

    let state = AppState {
        config: config.clone(),
        orchestrator: orchestrator.clone(),
        vector_store: vector_store.clone(),
        memory: memory.clone(),
    };

    // Check if CLI interactive mode requested
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--cli" || arg == "-c") {
        info!("Launching interactive CLI test mode...");
        run_cli_test_mode(orchestrator, vector_store, memory, config).await?;
        return Ok(());
    }

    // Build Axum WebServer routes
    let app = Router::new()
        .route("/evolution/webhook", post(handle_evolution_webhook))
        .route("/health", get(health_check))
        .with_state(state);

    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("maat Webhook Server listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Evolution API Incoming Webhook Handler
async fn handle_evolution_webhook(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let event = payload["event"].as_str().unwrap_or("");
    if event == "messages.upsert" || event == "MESSAGES_UPSERT" {
        let data = &payload["data"];
        let key = &data["key"];

        let from_me = key["fromMe"].as_bool().unwrap_or(false);
        if from_me {
            return StatusCode::OK; // Ignore messages sent by the bot itself
        }

        let remote_jid = key["remoteJid"].as_str().unwrap_or("");
        if remote_jid.ends_with("@g.us") || remote_jid.is_empty() {
            return StatusCode::OK; // Ignore group messages
        }

        let sender_phone = Config::normalize_phone(remote_jid.split('@').next().unwrap_or(""));
        let msg_id = key["id"].as_str().unwrap_or("evo_msg").to_string();

        let message_obj = &data["message"];
        let text_body = if let Some(c) = message_obj["conversation"].as_str() {
            c.to_string()
        } else if let Some(t) = message_obj["extendedTextMessage"]["text"].as_str() {
            t.to_string()
        } else {
            String::new()
        };

        let is_audio = message_obj.get("audioMessage").is_some();
        let msg_type = if is_audio { "audio" } else { "text" };

        info!("Incoming WhatsApp message from [{}], type={}, body: '{}'", sender_phone, msg_type, text_body);

        if !text_body.is_empty() || is_audio {
            let msg = models::IncomingMessage {
                from: sender_phone.clone(),
                id: msg_id,
                timestamp: chrono::Utc::now().timestamp().to_string(),
                msg_type: msg_type.to_string(),
                text: if !text_body.is_empty() {
                    Some(models::MessageText { body: text_body })
                } else {
                    None
                },
                audio: None,
                interactive: None,
            };

            let orch = state.orchestrator.clone();
            tokio::spawn(async move {
                if let Err(e) = orch.process_incoming_message(msg).await {
                    error!("Error processing message from {}: {}", sender_phone, e);
                }
            });
        }
    }

    StatusCode::OK
}

/// Health check route
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "service": "maat",
        "version": "2.0.0"
    })))
}

/// Interactive CLI harness for instant local testing without webhook dependencies
async fn run_cli_test_mode(
    orchestrator: Arc<AgentOrchestrator>,
    vector_store: Arc<VectorStore>,
    memory: Arc<ConversationMemory>,
    config: Config,
) -> anyhow::Result<()> {
    println!("\n=== maat (ماعت) CLI Interactive Testing Mode 2.0 ===");
    println!("Manager Phone (Sudo Mode): {}", config.manager_phone);
    println!("Commands:");
    println!("  /manager <text>        - Simulate Manager query (Zero Emoji Sudo Mode)");
    println!("  /worker <phone> <text> - Simulate Worker message/application");
    println!("  /rag <query>           - Test vector store retrieval");
    println!("  /profile <phone>       - View stored profile and history for a phone");
    println!("  /exit                  - Exit CLI mode\n");

    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();

    loop {
        print!("maat-cli> ");
        io::stdout().flush()?;
        line.clear();
        if handle.read_line(&mut line)? == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed == "/exit" || trimmed == "exit" {
            println!("Exiting CLI test mode.");
            break;
        }

        if trimmed.starts_with("/manager ") {
            let query = &trimmed[9..];
            let msg = models::IncomingMessage {
                from: config.manager_phone.clone(),
                id: format!("cli_{}", uuid::Uuid::new_v4()),
                timestamp: chrono::Utc::now().timestamp().to_string(),
                msg_type: "text".to_string(),
                text: Some(models::MessageText { body: query.to_string() }),
                audio: None,
                interactive: None,
            };
            orchestrator.process_incoming_message(msg).await?;
        } else if trimmed.starts_with("/worker ") {
            let parts: Vec<&str> = trimmed[8..].splitn(2, ' ').collect();
            if parts.len() == 2 {
                let phone = parts[0];
                let text = parts[1];
                let msg = models::IncomingMessage {
                    from: phone.to_string(),
                    id: format!("cli_{}", uuid::Uuid::new_v4()),
                    timestamp: chrono::Utc::now().timestamp().to_string(),
                    msg_type: "text".to_string(),
                    text: Some(models::MessageText { body: text.to_string() }),
                    audio: None,
                    interactive: None,
                };
                orchestrator.process_incoming_message(msg).await?;
            } else {
                println!("Usage: /worker <phone> <text>");
            }
        } else if trimmed.starts_with("/rag ") {
            let query = &trimmed[5..];
            let ctx = vector_store.get_rag_context(query).await;
            println!("\n--- RAG Vector Context ---\n{}\n--------------------------", ctx);
        } else if trimmed.starts_with("/profile ") {
            let phone = &trimmed[9..];
            let summary = memory.get_user_summary(phone);
            let history = memory.get_history(phone, 10);
            println!("\n--- User Profile [{}] ---", phone);
            println!("Summary: {}", summary);
            println!("Recent History ({} msgs):", history.len());
            for m in history {
                println!("  [{}] {}: {}", m.timestamp, m.role, m.content);
            }
            println!("--------------------------\n");
        } else if !trimmed.is_empty() {
            println!("Unknown command. Try /manager <text>, /worker <phone> <text>, /rag <query>, /profile <phone>, or /exit");
        }
    }

    Ok(())
}
