use std::sync::Mutex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType")]
    pub media_type: String,  
    pub config: Config,
    pub layers: Vec<Layer>,
    pub annotations: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Serialize, Deserialize)]
pub struct Layer {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Serialize, Deserialize)]
pub struct ManifestMetadata {
    #[serde(rename = "apiVersion")]
    pub api_version: String,  
    pub kind: String,       
    pub metadata: Metadata,
    pub spec: Spec,
}

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub annotations: Annotations,
}

#[derive(Serialize, Deserialize)]
pub struct Annotations {
    pub description: String,
    pub version: String,
    pub label: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub ui: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Spec {
    #[serde(rename = "type")]
    pub type_field: String,
    pub properties: Properties,
}

#[derive(Serialize, Deserialize)]
pub struct Properties {
    pub parameters: Parameters,
}

#[derive(Serialize, Deserialize)]
pub struct Parameters {
    #[serde(rename = "validation_schema")]
    pub validation_schema: ValidationSchema,
}

#[derive(Serialize, Deserialize)]
pub struct ValidationSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: Option<serde_json::Map<String, Value>>, 
    pub required: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct ComponentResponse {
    pub config: Option<serde_json::Value>,
    pub manifest: Option<Manifest>,
    pub wasm_binary: Option<String>,
   
}

pub struct ZotConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

pub struct AppState {
    pub zot_config: ZotConfig,
    pub client: Mutex<Client>,
}