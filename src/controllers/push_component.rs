use actix_multipart::Multipart;
use actix_web::{post, web, HttpResponse, Responder};
use chrono::Utc;
use futures::StreamExt;
use serde_json::Value;
use std::fs;

use crate::entities::{AppState, Config, Layer, Manifest, ManifestMetadata};
use crate::services::{calculate_sha256, init_upload, upload_blob};

#[post("/api/v1/components")]
pub async fn push_component(mut payload: Multipart, state: web::Data<AppState>) -> impl Responder {
    let mut manifest: Option<ManifestMetadata> = None;
    let mut wasm_file: Option<(String, Vec<u8>)> = None;

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(e) => return HttpResponse::BadRequest().body(format!("Erreur multipart: {}", e)),
        };

        let filename = match field.content_disposition().get_filename() {
            Some(name) => name.to_string(),
            None => continue,
        };

        let mut content = Vec::new();
        while let Some(chunk) = field.next().await {
            content.extend_from_slice(&chunk.unwrap());
        }

        if filename.ends_with(".json") {
            manifest = match serde_json::from_slice(&content) {
                Ok(m) => Some(m),
                Err(e) => {
                    return HttpResponse::BadRequest().body(format!("Erreur manifest: {}", e))
                }
            };
        } else if filename.ends_with(".wasm") {
            let path = format!("/tmp/{}", filename);
            fs::write(&path, &content).unwrap();
            wasm_file = Some((path, content));
        }
    }

    let manifest = match manifest {
        Some(m) => m,
        None => return HttpResponse::BadRequest().body("Manifest.json manquant"),
    };
    let (wasm_path, wasm_content) = match wasm_file {
        Some(f) => f,
        None => return HttpResponse::BadRequest().body("Fichier .wasm manquant"),
    };

    let layer_digest = calculate_sha256(&wasm_content);
    let config_content = serde_json::to_vec(&manifest).unwrap();
    let config_digest = calculate_sha256(&config_content);

    let client = state.client.lock().unwrap();

    let layer_upload_url = match init_upload(
        &client,
        &state.zot_config.url,
        &manifest.metadata.name,
        &state.zot_config.username,
        &state.zot_config.password,
    )
    .await
    {
        Ok(url) => url,
        Err(e) => return HttpResponse::InternalServerError().body(e),
    };

    if let Err(e) = upload_blob(
        &client,
        &layer_upload_url,
        &state.zot_config.username,
        &state.zot_config.password,
        &wasm_content,
        &layer_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    let config_upload_url = match init_upload(
        &client,
        &state.zot_config.url,
        &manifest.metadata.name,
        &state.zot_config.username,
        &state.zot_config.password,
    )
    .await
    {
        Ok(url) => url,
        Err(e) => return HttpResponse::InternalServerError().body(e),
    };

    if let Err(e) = upload_blob(
        &client,
        &config_upload_url,
        &state.zot_config.username,
        &state.zot_config.password,
        &config_content,
        &config_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    let mut annotations: serde_json::Map<String, Value> = serde_json::Map::new();
    annotations.insert(
        "org.opencontainers.image.title".to_string(),
        Value::String(manifest.metadata.name.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        Value::String(manifest.metadata.annotations.description.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.version".to_string(),
        Value::String(manifest.metadata.annotations.version.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.created".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );
    annotations.insert(
        "org.opencontainers.image.architecture".to_string(),
        Value::String("wasm".to_string()),
    );
    annotations.insert(
        "org.opencontainers.image.os".to_string(),
        Value::String("any".to_string()),
    );
    if let Some(label) = &manifest.metadata.annotations.label {
        annotations.insert(
            "org.opencontainers.image.label".to_string(),
            Value::String(label.clone()),
        );
    }
    if let Some(icon) = &manifest.metadata.annotations.icon {
        annotations.insert(
            "org.opencontainers.image.icon".to_string(),
            Value::String(icon.clone()),
        );
    }
    if let Some(color) = &manifest.metadata.annotations.color {
        annotations.insert(
            "org.opencontainers.image.color".to_string(),
            Value::String(color.clone()),
        );
    }
    if let Some(ui) = &manifest.metadata.annotations.ui {
        annotations.insert(
            "org.opencontainers.image.ui".to_string(),
            Value::String(ui.clone()),
        );
    }
    annotations.insert(
        "com.aneocorp.component.type".to_string(),
        Value::String(manifest.spec.type_field.clone()),
    );

    let manifest_data = Manifest {
        schema_version: 2,
        media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
        config: Config {
            media_type: "application/vnd.oci.image.config.v1+json".to_string(),
            size: config_content.len() as i64,
            digest: config_digest,
        },
        layers: vec![Layer {
            media_type: "application/wasm".to_string(),
            size: wasm_content.len() as i64,
            digest: layer_digest,
        }],
        annotations: Some(Value::Object(annotations)),
    };

    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        state.zot_config.url, manifest.metadata.name, manifest.metadata.annotations.version
    );

    let response = client
        .put(&manifest_url)
        .basic_auth(&state.zot_config.username, Some(&state.zot_config.password))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .json(&manifest_data)
        .send()
        .await;

    if !wasm_path.is_empty() {
        fs::remove_file(&wasm_path).unwrap();
    }

    match response {
        Ok(resp) if resp.status().is_success() => HttpResponse::Ok().body("Upload réussi!"),
        Ok(resp) => HttpResponse::BadRequest().body(format!("Erreur manifest: {}", resp.status())),
        Err(e) => HttpResponse::InternalServerError().body(format!("Erreur: {}", e)),
    }
}
