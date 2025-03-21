use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub schemaVersion: i32,
    pub mediaType: String,
    pub config: Config,
    pub layers: Vec<Layer>,
    pub annotations: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub mediaType: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Serialize, Deserialize)]
pub struct Layer {
    pub mediaType: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Deserialize)]
pub struct ManifestMetadata {
    pub name: String,
    pub architecture: String,
    pub os: String,
    pub description: String,
    pub author: String,
    pub tag: String,
    pub url: Option<String>,
    pub source: Option<String>,
    pub revision: Option<String>,
    pub licenses: Option<String>,
    pub vendor: Option<String>,
    pub documentation: Option<String>,
    pub component_type: Option<String>,
}

#[derive(Serialize)]
pub struct ComponentResponse {
    pub manifest: Option<Manifest>,
    pub wasm_binary: Option<String>,
}

pub struct ZotConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}