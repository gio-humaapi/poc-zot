use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub schemaVersion: i32,
    pub mediaType: String,
    pub config: Config,
    pub layers: Vec<Layer>,
    pub annotations: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    pub mediaType: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Serialize)]
pub(crate) struct ComponentResponse {
    pub manifest: Option<Manifest>,
    pub wasm_binary: Option<String>, // Base64 encoded binary
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Layer {
    pub mediaType: String,
    pub size: i64,
    pub digest: String,
}

#[derive(Deserialize)]
pub(crate) struct ManifestMetadata {
    pub name: String,
    pub architecture: String,
    pub os: String,
    pub description: String,
    pub author: String,
    pub tag: String,
}

pub(crate) struct ZotConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}