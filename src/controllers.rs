use std::fs;
use actix_web::{get, post, put, HttpResponse, Responder};
use actix_multipart::Multipart;
use base64::Engine;
use futures::StreamExt;
use reqwest::Client;
use crate::entities::{ComponentResponse, Config, Layer, Manifest, ManifestMetadata, ZotConfig};
use crate::services::{calculate_sha256, init_upload, upload_blob};

#[post("/api/v1/components")]
async fn push_component(mut payload: Multipart) -> impl Responder {
    let client = Client::new();
    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

    let mut manifest: Option<ManifestMetadata> = None;
    let mut wasm_file: Option<(String, Vec<u8>)> = None;

    // Traitement du payload multipart
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

    // Calcul des digests
    let layer_digest = calculate_sha256(&wasm_content);
    let config_content = b"{}";
    let config_digest = calculate_sha256(config_content);

    // Upload du layer
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
        &wasm_content, // Passé par référence
        &layer_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    // Upload de la config
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
        &config_content.to_vec(), // Converti en Vec pour compatibilité
        &config_digest,
    )
    .await
    {
        return HttpResponse::InternalServerError().body(e);
    }

    // Création et envoi du manifest
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
        annotations: Some(serde_json::json!({
            "org.opencontainers.image.title": manifest.name,
            "org.opencontainers.image.architecture": manifest.architecture,
            "org.opencontainers.image.os": manifest.os,
            "org.opencontainers.image.description": manifest.description,
            "org.opencontainers.image.authors": manifest.author,
            "org.opencontainers.image.version": manifest.tag 
        })),
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
async fn get_component(
    path: actix_web::web::Path<(String, String)>,
) -> impl Responder {
    let (repository, reference) = path.into_inner();
    let client = Client::new();
    let zot_config = ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "bot".to_string(),
        password: "helptheworld".to_string(),
    };

    // Étape 1 : Récupérer le manifest
    let manifest_url = format!("{}/v2/{}/manifests/{}", zot_config.url, repository, reference);
    let manifest_response = client
        .get(&manifest_url)
        .basic_auth(&zot_config.username, Some(&zot_config.password))
        .header("Accept", "application/vnd.oci.image.manifest.v1+json")
        .send()
        .await;

    let manifest = match manifest_response {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Manifest>().await {
                Ok(m) => Some(m),
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .body(format!("Erreur parsing manifest: {}", e))
                }
            }
        }
        Ok(resp) => {
            return HttpResponse::NotFound()
                .body(format!("Manifest non trouvé: {}", resp.status()))
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .body(format!("Erreur récupération manifest: {}", e))
        }
    };

    // Étape 2 : Récupérer le binaire WASM si disponible dans le manifest
    let wasm_binary = if let Some(ref manifest) = manifest {
        if let Some(layer) = manifest.layers.first() {
            if layer.mediaType == "application/wasm" {
                let blob_url = format!("{}/v2/{}/blobs/{}", zot_config.url, repository, layer.digest);
                let blob_response = client
                    .get(&blob_url)
                    .basic_auth(&zot_config.username, Some(&zot_config.password))
                    .send()
                    .await;

                match blob_response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.bytes().await {
                            Ok(bytes) => {
                                let engine = base64::engine::general_purpose::STANDARD;
                                Some(engine.encode(bytes))
                            },
                            Err(e) => {
                                return HttpResponse::InternalServerError()
                                    .body(format!("Erreur lecture binaire: {}", e))
                            }
                        }
                    }
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

    // Construire la réponse
    let response = ComponentResponse {
        manifest,
        wasm_binary,
    };

    HttpResponse::Ok().json(response)
}

#[put("/api/v1/{repository}/components/{reference}")]
async fn update_component(
    path: actix_web::web::Path<(String, String)>,
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

    // Traitement du payload multipart
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

    // Vérification des données fournies
    let manifest = match manifest {
        Some(m) => m,
        None => return HttpResponse::BadRequest().body("Manifest.json manquant"),
    };

    // Si aucun nouveau binaire n'est fourni, on ne met à jour que le manifest
    let (wasm_path, wasm_content) = match wasm_file {
        Some(f) => f,
        None => (String::new(), Vec::new()), // Valeurs par défaut si pas de binaire
    };

    // Calcul des digests
    let layer_digest = if !wasm_content.is_empty() {
        calculate_sha256(&wasm_content)
    } else {
        // On pourrait récupérer l'ancien digest ici si nécessaire, pour l'instant on échoue
        return HttpResponse::BadRequest().body("Aucun nouveau binaire fourni et digest actuel non géré");
    };
    let config_content = b"{}"; // Config minimale
    let config_digest = calculate_sha256(config_content);

    // Upload du nouveau binaire s'il est fourni
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

    // Upload de la config (toujours mis à jour pour simplifier)
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

    // Création du nouveau manifest
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
            // Si pas de nouveau binaire, on pourrait récupérer l'ancien layer ici
            vec![]
        },
        annotations: Some(serde_json::json!({
            "org.opencontainers.image.title": manifest.name,
            "org.opencontainers.image.architecture": manifest.architecture,
            "org.opencontainers.image.os": manifest.os,
            "org.opencontainers.image.description": manifest.description,
            "org.opencontainers.image.authors": manifest.author,
            "org.opencontainers.image.version": manifest.tag // Ajout de la version ici
        })),
    };

    // Mise à jour du manifest dans Zot
    let manifest_url = format!("{}/v2/{}/manifests/{}", zot_config.url, repository, reference);
    let response = client
        .put(&manifest_url)
        .basic_auth(&zot_config.username, Some(&zot_config.password))
        .header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
        .json(&manifest_data)
        .send()
        .await;

    // Nettoyage si un fichier temporaire a été créé
    if !wasm_path.is_empty() {
        fs::remove_file(&wasm_path).unwrap();
    }
    
    match response {
        Ok(resp) if resp.status().is_success() => HttpResponse::Ok().body("Mise à jour réussie!"),
        Ok(resp) => HttpResponse::BadRequest().body(format!("Erreur mise à jour: {}", resp.status())),
        Err(e) => HttpResponse::InternalServerError().body(format!("Erreur: {}", e)),
    }
}