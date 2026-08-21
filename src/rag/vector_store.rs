use crate::models::DocumentRecord;
use reqwest::Client;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use turbovec::TurboQuantIndex;
use uuid::Uuid;

pub struct VectorStore {
    documents: Arc<RwLock<Vec<DocumentRecord>>>,
    index: Arc<RwLock<Option<TurboQuantIndex>>>,
    mistral_api_key: String,
    http_client: Client,
    storage_path: String,
}

impl VectorStore {
    pub fn new(mistral_api_key: String) -> Self {
        let storage_path = "maat_vectors.json".to_string();
        let mut initial_docs = Vec::new();

        let path = Path::new(&storage_path);
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(path) {
                if let Ok(docs) = serde_json::from_str::<Vec<DocumentRecord>>(&data) {
                    info!("Loaded {} existing documents from vector store disk.", docs.len());
                    initial_docs = docs;
                }
            }
        }

        Self {
            documents: Arc::new(RwLock::new(initial_docs)),
            index: Arc::new(RwLock::new(None)),
            mistral_api_key,
            http_client: Client::new(),
            storage_path,
        }
    }

    /// Save current documents to disk for persistence
    async fn save_to_disk(&self) {
        let docs = self.documents.read().await;
        if let Ok(json_str) = serde_json::to_string_pretty(&*docs) {
            let _ = std::fs::write(&self.storage_path, json_str);
        }
    }

    /// Compute embedding vector using Mistral Embed API (fallback to normalized semantic vector)
    pub async fn get_embedding(&self, text: &str) -> Vec<f32> {
        if !self.mistral_api_key.is_empty() {
            let res = self
                .http_client
                .post("https://api.mistral.ai/v1/embeddings")
                .header("Authorization", format!("Bearer {}", self.mistral_api_key))
                .header("Content-Type", "application/json")
                .json(&json!({
                    "model": "mistral-embed",
                    "input": [text]
                }))
                .send()
                .await;

            if let Ok(resp) = res {
                if let Ok(json_val) = resp.json::<serde_json::Value>().await {
                    if let Some(embedding_arr) = json_val["data"][0]["embedding"].as_array() {
                        let vec_f32: Vec<f32> = embedding_arr
                            .iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect();
                        if !vec_f32.is_empty() {
                            return vec_f32;
                        }
                    }
                }
            } else {
                error!("Mistral embedding API call failed, falling back to internal embedding vector.");
            }
        }

        Self::fallback_embedding(text, 1024)
    }

    fn fallback_embedding(text: &str, dim: usize) -> Vec<f32> {
        let mut vec = vec![0.0f32; dim];
        for (i, b) in text.bytes().enumerate() {
            let idx = (b as usize + i * 31) % dim;
            vec[idx] += 1.0;
        }
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }
        vec
    }

    /// Add a new document to the RAG vector store and index it with Turbovec
    pub async fn add_document(&self, doc_type: &str, phone: &str, title: &str, content: &str) -> String {
        let id = Uuid::new_v4().to_string();
        let embedding = self.get_embedding(content).await;

        let doc = DocumentRecord {
            id: id.clone(),
            doc_type: doc_type.to_string(),
            phone_number: phone.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            embedding,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        {
            let mut docs = self.documents.write().await;
            docs.push(doc.clone());
            self.rebuild_index(&docs).await;
        }

        self.save_to_disk().await;
        info!("Added document [{}] type={} to RAG Turbovec Store.", id, doc_type);
        id
    }

    async fn rebuild_index(&self, docs: &[DocumentRecord]) {
        if docs.is_empty() {
            return;
        }

        let dim = docs[0].embedding.len();
        let mut flat_vectors: Vec<f32> = Vec::new();
        for d in docs {
            if d.embedding.len() == dim {
                flat_vectors.extend_from_slice(&d.embedding);
            }
        }

        if !flat_vectors.is_empty() {
            if let Ok(mut idx) = TurboQuantIndex::new(dim, 4) {
                idx.add(&flat_vectors);
                let mut index_guard = self.index.write().await;
                *index_guard = Some(idx);
            }
        }
    }

    /// Perform RAG vector similarity search
    pub async fn search_similar(&self, query: &str, top_k: usize) -> Vec<DocumentRecord> {
        let query_embedding = self.get_embedding(query).await;
        let docs = self.documents.read().await;

        if docs.is_empty() {
            return Vec::new();
        }

        let mut scored_docs: Vec<(f32, DocumentRecord)> = docs
            .iter()
            .map(|doc| {
                let score = cosine_similarity(&query_embedding, &doc.embedding);
                (score, doc.clone())
            })
            .collect();

        // Sort descending by similarity score
        scored_docs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored_docs
            .into_iter()
            .take(top_k)
            .map(|(_, doc)| doc)
            .collect()
    }

    /// Search specifically for documents related to a specific phone number or user name
    pub async fn search_by_phone_or_keyword(&self, phone: &str, keyword: &str) -> Vec<DocumentRecord> {
        let docs = self.documents.read().await;
        let clean_phone = phone.chars().filter(|c| c.is_ascii_digit()).collect::<String>();

        docs.iter()
            .filter(|d| {
                d.phone_number.contains(&clean_phone)
                    || d.content.contains(keyword)
                    || d.title.contains(keyword)
            })
            .cloned()
            .collect()
    }

    /// Format documents into RAG context string for the LLM
    pub async fn get_rag_context(&self, query: &str) -> String {
        let results = self.search_similar(query, 5).await;
        if results.is_empty() {
            return "لا توجد بيانات مسجلة مطابقة حتى الآن.".to_string();
        }

        let mut context = String::from("البيانات والسجلات المسترجعة من قاعدة بيانات ماعت (Turbovec RAG):\n");
        for (idx, doc) in results.iter().enumerate() {
            context.push_str(&format!(
                "{}. [{}] [تاريخ: {}] [رقم/عامل: {}]: {}\n",
                idx + 1,
                doc.title,
                doc.timestamp,
                doc.phone_number,
                doc.content
            ));
        }
        context
    }
}

fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.len() != v2.len() || v1.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (a, b) in v1.iter().zip(v2.iter()) {
        dot += a * b;
        norm_a += a * a;
        norm_b += b * b;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a.sqrt() * norm_b.sqrt())
    }
}
