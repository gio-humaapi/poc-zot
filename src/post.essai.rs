use actix_multipart::Multipart;
use actix_web::{post, App, HttpResponse, HttpServer, Responder};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::read, io::Write};

#[derive(Serialize)]
struct Manifest {
    schemaVersion: i32,
    mediaType: String,
    config: Config,
    layers: Vec<Layer>,
    annotations: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct Config {
    mediaType: String,
    size: i64,
    digest: String,
}

#[derive(Serialize)]
struct Layer {
    mediaType: String,
    size: i64,
    digest: String,
}

#[derive(Deserialize)]
struct ManifestMetadata {
    name: String,
    architecture: String,
    os: String,
    description: String,
    author: String,
    tag: String,
}

struct ZotConfig {
    url: String,
    username: String,
    password: String,
}

#[post("/api/v1/components")]
async fn push_component(mut payload: Multipart) -> impl Responder {
    let client = Client::new();

    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

    let mut manifest: Option<ManifestMetadata> = None;
    let mut wasm_file_path: Option<String> = None;
    let mut wasm_file_size: i32 = 0;
    while let Some(item) = payload.next().await {
        match item {
            Ok(mut field) => {
                let content_type = field.content_disposition();
                if let Some(filename) = content_type.get_filename() {
                    if filename.ends_with(".json") {
                        // Lire le fichier manifest.json sans connaître son nom
                        let mut manifest_content = Vec::new();
                        while let Some(chunk) = field.next().await {
                            let data = chunk.unwrap();
                            manifest_content.extend_from_slice(&data);
                        }

                        // Parser le contenu JSON
                        match serde_json::from_slice::<ManifestMetadata>(&manifest_content) {
                            Ok(parsed_manifest) => manifest = Some(parsed_manifest),
                            Err(e) => {
                                return HttpResponse::BadRequest()
                                    .body(format!("Erreur parsing manifest : {}", e))
                            }
                        }
                    } else if filename.ends_with(".wasm") {
                        // Stocker temporairement le fichier wasm
                        let wasm_path = format!("/tmp/{}", filename);
                        let mut file = std::fs::File::create(&wasm_path).unwrap();
                        while let Some(chunk) = field.next().await {
                            let data = chunk.unwrap();
                            wasm_file_size += data.len() as i32;
                            file.write_all(&data).unwrap();
                        }
                        wasm_file_path = Some(wasm_path);
                    }
                }
            }
            Err(e) => return HttpResponse::BadRequest().body(format!("Erreur multipart : {}", e)),
        }
    }
    // Vérifier que le manifest et le fichier wasm ont bien été reçus
    let manifest = match manifest {
        Some(m) => m,
        None => return HttpResponse::BadRequest().body("Manifest.json manquant"),
    };
    let wasm_file_path = match wasm_file_path {
        Some(path) => path,
        None => return HttpResponse::BadRequest().body("Fichier .wasm manquant"),
    };

    let name = manifest.name;
    let architecture = manifest.architecture;
    let os = manifest.os;
    let description = manifest.description;
    let author = manifest.author;
    let tag = manifest.tag;

    // Calculer le digest SHA256 du layer
    let mut hasher = Sha256::new();
    hasher.update(read(&wasm_file_path).unwrap());
    let layer_digest = format!("sha256:{}", hex::encode(hasher.finalize()));

    // Étape 1 : Initier l'upload du layer
    let init_url = format!("{}/v2/{}/blobs/uploads/", zot_config.url, name);
    let init_response = client
        .post(&init_url)
        .basic_auth(
            zot_config.username.clone(),
            Some(zot_config.password.clone()),
        )
        .send()
        .await;

    let layer_upload_url = match init_response {
        Ok(resp) if resp.status().is_success() => {
            let location = resp
                .headers()
                .get("Location")
                .and_then(|loc| loc.to_str().ok())
                .unwrap_or_default();
            if location.starts_with("http") {
                location.to_string()
            } else {
                format!("{}{}", zot_config.url, location)
            }
        }
        Ok(resp) => {
            return HttpResponse::BadRequest()
                .body(format!("Erreur init upload layer : {}", resp.status()));
        }
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Erreur init layer : {}", e));
        }
    };

    // Étape 2 : Envoyer le blob du layer
    let file_content = std::fs::read(&wasm_file_path).unwrap(); // Read the file content
    let blob_response = client
        .put(&layer_upload_url)
        .basic_auth(
            zot_config.username.clone(),
            Some(zot_config.password.clone()),
        )
        .query(&[("digest", &layer_digest)])
        .body(file_content)
        .header("Content-Type", "application/octet-stream")
        .send()
        .await;

    if let Err(e) = blob_response {
        return HttpResponse::InternalServerError()
            .body(format!("Erreur blob upload layer : {}", e));
    }
    let blob_resp = blob_response.unwrap();
    if !blob_resp.status().is_success() {
        return HttpResponse::BadRequest()
            .body(format!("Erreur blob layer : {}", blob_resp.status()));
    }

    // Créer et uploader une config minimale
    let config_content = b"{}"; // JSON vide comme config
    let config_size = config_content.len() as i64; // Calculer la taille de la config
    let mut hasher = Sha256::new();
    hasher.update(config_content);
    let config_digest = format!("sha256:{}", hex::encode(hasher.finalize()));

    // Étape 3 : Initier l'upload de la config
    let config_init_response = client
        .post(&init_url)
        .basic_auth(
            zot_config.username.clone(),
            Some(zot_config.password.clone()),
        )
        .send()
        .await;

    let config_upload_url = match config_init_response {
        Ok(resp) if resp.status().is_success() => {
            let location = resp
                .headers()
                .get("Location")
                .and_then(|loc| loc.to_str().ok())
                .unwrap_or_default();
            if location.starts_with("http") {
                location.to_string()
            } else {
                format!("{}{}", zot_config.url, location)
            }
        }
        Ok(resp) => {
            return HttpResponse::BadRequest()
                .body(format!("Erreur init upload config : {}", resp.status()));
        }
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Erreur init config : {}", e));
        }
    };

    // Étape 4 : Envoyer le blob de la config
    let config_blob_response = client
        .put(&config_upload_url)
        .basic_auth(
            zot_config.username.clone(),
            Some(zot_config.password.clone()),
        )
        .query(&[("digest", &config_digest)])
        .body(config_content.to_vec())
        .header("Content-Type", "application/octet-stream")
        .send()
        .await;

    if let Err(e) = config_blob_response {
        return HttpResponse::InternalServerError()
            .body(format!("Erreur blob upload config : {}", e));
    }
    let config_blob_resp = config_blob_response.unwrap();
    if !config_blob_resp.status().is_success() {
        return HttpResponse::BadRequest().body(format!(
            "Erreur blob config : {}",
            config_blob_resp.status()
        ));
    }

    // Étape 5 : Créer et envoyer le manifest avec PUT
    let manifest = Manifest {
        schemaVersion: 2,
        mediaType: "application/vnd.oci.image.manifest.v1+json".to_string(),
        config: Config {
            mediaType: "application/vnd.oci.image.config.v1+json".to_string(),
            size: config_size,
            digest: config_digest,
        },
        layers: vec![Layer {
            mediaType: "application/wasm".to_string(),
            size: wasm_file_size as i64,
            digest: layer_digest,
        }],
        annotations: Some(serde_json::json!({
            "org.opencontainers.image.title": name,
            "org.opencontainers.image.architecture": architecture,
            "org.opencontainers.image.os": os,
            "org.opencontainers.image.description": description,
            "org.opencontainers.image.authors": author,
        })),
    };

    let manifest_url = format!("{}/v2/{}/manifests/{}", zot_config.url, name, tag);
    let manifest_response = client
        .put(&manifest_url)
        .basic_auth(zot_config.username.clone(), Some(zot_config.password.clone()))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .json(&manifest)
        .send()
        .await;

    match manifest_response {
        Ok(resp) if resp.status().is_success() => {
            std::fs::remove_file(&wasm_file_path).unwrap();
            return HttpResponse::Ok().body("Fichier envoyé à Zot avec succès !");
        }
        Ok(resp) => {
            return HttpResponse::BadRequest().body(format!("Erreur manifest : {}", resp.status()));
        }
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("Erreur manifest : {}", e));
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Serveur démarré sur http://localhost:8080");
    HttpServer::new(|| App::new().service(push_component))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
