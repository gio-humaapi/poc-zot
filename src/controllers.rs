use crate::entities::{ComponentResponse, Config, Layer, Manifest, ManifestMetadata, ZotConfig};
use crate::services::{calculate_sha256, init_upload, upload_blob};
use actix_multipart::Multipart;
use actix_web::{get, post, put, web, HttpResponse, Responder};
use base64::Engine;
use chrono::Utc;
use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::fs;

#[post("/api/v1/components")]
pub async fn push_component(mut payload: Multipart) -> impl Responder {
    let client = Client::new();
    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

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
    let config_content = b"{}";
    let config_digest = calculate_sha256(config_content);

    let layer_upload_url = match init_upload(
        &client,
        &zot_config.url,
        &manifest.name,
        &zot_config.username,
        &zot_config.password,
    )
    .await
    {
        Ok(url) => url,
        Err(e) => return HttpResponse::InternalServerError().body(e),
    };

    if let Err(e) = upload_blob(
        &client,
        &layer_upload_url,
        &zot_config.username,
        &zot_config.password,
        &wasm_content,
        &layer_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    let config_upload_url = match init_upload(
        &client,
        &zot_config.url,
        &manifest.name,
        &zot_config.username,
        &zot_config.password,
    )
    .await
    {
        Ok(url) => url,
        Err(e) => return HttpResponse::InternalServerError().body(e),
    };

    if let Err(e) = upload_blob(
        &client,
        &config_upload_url,
        &zot_config.username,
        &zot_config.password,
        &config_content.to_vec(),
        &config_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    let mut annotations: serde_json::Map<String, Value> = serde_json::Map::new();
    annotations.insert(
        "org.opencontainers.image.title".to_string(),
        Value::String(manifest.name.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.architecture".to_string(),
        Value::String(manifest.architecture.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.os".to_string(),
        Value::String(manifest.os.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        Value::String(manifest.description.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.author".to_string(),
        Value::String(manifest.author.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.version".to_string(),
        Value::String(manifest.tag.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.created".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );

    if let Some(url) = &manifest.url {
        annotations.insert(
            "org.opencontainers.image.url".to_string(),
            Value::String(url.clone()),
        );
    }
    if let Some(source) = &manifest.source {
        annotations.insert(
            "org.opencontainers.image.source".to_string(),
            Value::String(source.clone()),
        );
    }
    if let Some(revision) = &manifest.revision {
        annotations.insert(
            "org.opencontainers.image.revision".to_string(),
            Value::String(revision.clone()),
        );
    }
    if let Some(licenses) = &manifest.licenses {
        annotations.insert(
            "org.opencontainers.image.licenses".to_string(),
            Value::String(licenses.clone()),
        );
    }
    if let Some(vendor) = &manifest.vendor {
        annotations.insert(
            "org.opencontainers.image.vendor".to_string(),
            Value::String(vendor.clone()),
        );
    }
    if let Some(documentation) = &manifest.documentation {
        annotations.insert(
            "org.opencontainers.image.documentation".to_string(),
            Value::String(documentation.clone()),
        );
    }
    if let Some(component_type) = &manifest.component_type {
        annotations.insert(
            "com.aneocorp.component.type".to_string(),
            Value::String(component_type.clone()),
        );
    }

    let manifest_data = Manifest {
        schemaVersion: 2,
        mediaType: "application/vnd.oci.image.manifest.v1+json".to_string(),
        config: Config {
            mediaType: "application/vnd.oci.image.config.v1+json".to_string(),
            size: config_content.len() as i64,
            digest: config_digest,
        },
        layers: vec![Layer {
            mediaType: "application/wasm".to_string(),
            size: wasm_content.len() as i64,
            digest: layer_digest,
        }],
        annotations: Some(Value::Object(annotations)),
    };

    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        zot_config.url, manifest.name, manifest.tag
    );

    let response = client
        .put(&manifest_url)
        .basic_auth(&zot_config.username, Some(&zot_config.password))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .json(&manifest_data)
        .send()
        .await;

    fs::remove_file(&wasm_path).unwrap();
    match response {
        Ok(resp) if resp.status().is_success() => HttpResponse::Ok().body("Upload réussi!"),
        Ok(resp) => HttpResponse::BadRequest().body(format!("Erreur manifest: {}", resp.status())),
        Err(e) => HttpResponse::InternalServerError().body(format!("Erreur: {}", e)),
    }
}

#[get("/api/v1/{repository}/components/{reference}")]
pub async fn get_component(path: web::Path<(String, String)>) -> impl Responder {
    let (repository, reference) = path.into_inner();
    let client = Client::new();
    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        zot_config.url, repository, reference
    );
    let manifest_response = client
        .get(&manifest_url)
        .basic_auth(&zot_config.username, Some(&zot_config.password))
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await;

    let manifest = match manifest_response {
        Ok(resp) if resp.status().is_success() => match resp.json::<Manifest>().await {
            Ok(m) => Some(m),
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Erreur parsing manifest: {}", e))
            }
        },
        Ok(resp) => {
            return HttpResponse::NotFound().body(format!("Manifest non trouvé: {}", resp.status()))
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Erreur récupération manifest: {}", e))
        }
    };

    let wasm_binary = if let Some(ref manifest) = manifest {
        if let Some(layer) = manifest.layers.first() {
            if layer.mediaType == "application/wasm" {
                let blob_url = format!(
                    "{}/v2/{}/blobs/{}",
                    zot_config.url, repository, layer.digest
                );
                let blob_response = client
                    .get(&blob_url)
                    .basic_auth(&zot_config.username, Some(&zot_config.password))
                    .send()
                    .await;

                match blob_response {
                    Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                        Ok(bytes) => Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                        Err(e) => {
                            return HttpResponse::InternalServerError()
                                .body(format!("Erreur lecture binaire: {}", e))
                        }
                    },
                    Ok(resp) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Erreur récupération binaire: {}", resp.status()))
                    }
                    Err(e) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Erreur requête binaire: {}", e))
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let response = ComponentResponse {
        manifest,
        wasm_binary,
    };

    HttpResponse::Ok().json(response)
}

#[put("/api/v1/{repository}/components/{reference}")]
pub async fn update_component(
    path: web::Path<(String, String)>,
    mut payload: Multipart,
) -> impl Responder {
    let (repository, reference) = path.into_inner();
    let client = Client::new();
    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

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
        None => (String::new(), Vec::new()),
    };

    let layer_digest = if !wasm_content.is_empty() {
        calculate_sha256(&wasm_content)
    } else {
        return HttpResponse::BadRequest().body("Aucun nouveau binaire fourni et digest actuel non géré");
    };
    let config_content = b"{}";
    let config_digest = calculate_sha256(config_content);

    if !wasm_content.is_empty() {
        let layer_upload_url = match init_upload(
            &client,
            &zot_config.url,
            &repository,
            &zot_config.username,
            &zot_config.password,
        )
        .await
        {
            Ok(url) => url,
            Err(e) => return HttpResponse::InternalServerError().body(e),
        };

        if let Err(e) = upload_blob(
            &client,
            &layer_upload_url,
            &zot_config.username,
            &zot_config.password,
            &wasm_content,
            &layer_digest,
        )
        .await
        {
            return HttpResponse::InternalServerError().body(e);
        }
    }

    let config_upload_url = match init_upload(
        &client,
        &zot_config.url,
        &repository,
        &zot_config.username,
        &zot_config.password,
    )
    .await
    {
        Ok(url) => url,
        Err(e) => return HttpResponse::InternalServerError().body(e),
    };

    if let Err(e) = upload_blob(
        &client,
        &config_upload_url,
        &zot_config.username,
        &zot_config.password,
        &config_content.to_vec(),
        &config_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    let mut annotations: serde_json::Map<String, Value> = serde_json::Map::new();
    annotations.insert(
        "org.opencontainers.image.title".to_string(),
        Value::String(manifest.name.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.architecture".to_string(),
        Value::String(manifest.architecture.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.os".to_string(),
        Value::String(manifest.os.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.description".to_string(),
        Value::String(manifest.description.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.author".to_string(),
        Value::String(manifest.author.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.version".to_string(),
        Value::String(manifest.tag.clone()),
    );
    annotations.insert(
        "org.opencontainers.image.created".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );

    if let Some(url) = &manifest.url {
        annotations.insert("org.opencontainers.image.url".to_string(), Value::String(url.clone()));
    }
    if let Some(source) = &manifest.source {
        annotations.insert("org.opencontainers.image.source".to_string(), Value::String(source.clone()));
    }
    if let Some(revision) = &manifest.revision {
        annotations.insert("org.opencontainers.image.revision".to_string(), Value::String(revision.clone()));
    }
    if let Some(licenses) = &manifest.licenses {
        annotations.insert("org.opencontainers.image.licenses".to_string(), Value::String(licenses.clone()));
    }
    if let Some(vendor) = &manifest.vendor {
        annotations.insert("org.opencontainers.image.vendor".to_string(), Value::String(vendor.clone()));
    }
    if let Some(documentation) = &manifest.documentation {
        annotations.insert("org.opencontainers.image.documentation".to_string(), Value::String(documentation.clone()));
    }
    if let Some(component_type) = &manifest.component_type {
        annotations.insert("com.aneocorp.component.type".to_string(), Value::String(component_type.clone()));
    }

    let manifest_data = Manifest {
        schemaVersion: 2,
        mediaType: "application/vnd.oci.image.manifest.v1+json".to_string(),
        config: Config {
            mediaType: "application/vnd.oci.image.config.v1+json".to_string(),
            size: config_content.len() as i64,
            digest: config_digest,
        },
        layers: if !wasm_content.is_empty() {
            vec![Layer {
                mediaType: "application/wasm".to_string(),
                size: wasm_content.len() as i64,
                digest: layer_digest,
            }]
        } else {
            vec![]
        },
        annotations: Some(Value::Object(annotations)),
    };

    let manifest_url = format!("{}/v2/{}/manifests/{}", zot_config.url, repository, reference);
    let response = client
        .put(&manifest_url)
        .basic_auth(&zot_config.username, Some(&zot_config.password))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .json(&manifest_data)
        .send()
        .await;

    if !wasm_path.is_empty() {
        fs::remove_file(&wasm_path).unwrap();
    }

    match response {
        Ok(resp) if resp.status().is_success() => HttpResponse::Ok().body("Mise à jour réussie!"),
        Ok(resp) => HttpResponse::BadRequest().body(format!("Erreur mise à jour: {}", resp.status())),
        Err(e) => HttpResponse::InternalServerError().body(format!("Erreur: {}", e)),
    }
}