use actix_web::{get, web, HttpResponse, Responder};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::entities::{AppState, ComponentResponse, Manifest};
use crate::services::calculate_sha256;

#[get("/api/v1/{repository}/components/{reference}")]
pub async fn get_component(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> impl Responder {
    let (repository, reference) = path.into_inner();
    let client = state.client.lock().unwrap();

    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        state.zot_config.url, repository, reference
    );
    let manifest_response = client
        .get(&manifest_url)
        .basic_auth(&state.zot_config.username, Some(&state.zot_config.password))
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
            if layer.media_type == "application/wasm" {
                let wasm_url = format!(
                    "{}/v2/{}/blobs/{}",
                    state.zot_config.url, repository, layer.digest
                );
                let wasm_response = client
                    .get(&wasm_url)
                    .basic_auth(&state.zot_config.username, Some(&state.zot_config.password))
                    .send()
                    .await;

                match wasm_response {
                    Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                        Ok(bytes) => Some(BASE64.encode(bytes)),
                        Err(e) => {
                            return HttpResponse::InternalServerError()
                                .body(format!("Erreur lecture WASM: {}", e))
                        }
                    },
                    Ok(resp) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Erreur récupération WASM: {}", resp.status()))
                    }
                    Err(e) => {
                        return HttpResponse::InternalServerError()
                            .body(format!("Erreur requête WASM: {}", e))
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

    let config_content = if let Some(ref manifest) = manifest {
        let config_url = format!(
            "{}/v2/{}/blobs/{}",
            state.zot_config.url, repository, manifest.config.digest
        );
        let config_response = client
            .get(&config_url)
            .basic_auth(&state.zot_config.username, Some(&state.zot_config.password))
            .send()
            .await;

        match config_response {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) => {
                    let calculated_digest = calculate_sha256(&bytes);
                    if calculated_digest != manifest.config.digest {
                        return HttpResponse::InternalServerError()
                            .body("Digest de la config ne correspond pas");
                    }
                    match serde_json::from_slice(&bytes) {
                        Ok(json) => Some(json),
                        Err(e) => {
                            return HttpResponse::InternalServerError()
                                .body(format!("Erreur parsing config JSON: {}", e))
                        }
                    }
                }
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .body(format!("Erreur lecture config: {}", e))
                }
            },
            Ok(resp) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Erreur récupération config: {}", resp.status()))
            }
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .body(format!("Erreur requête config: {}", e))
            }
        }
    } else {
        None
    };

    let response = ComponentResponse {
        manifest,
        wasm_binary,
        config: config_content,
    };

    HttpResponse::Ok().json(response)
}
