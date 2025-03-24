use actix_web::{delete, web, HttpResponse, Responder};

use crate::entities::AppState;

#[delete("/api/v1/{repository}/components/{reference}")]
pub async fn delete_component(
    path: web::Path<(String, String)>,
    state: web::Data<AppState>,
) -> impl Responder {
    let (repository, reference) = path.into_inner();
    let client = state.client.lock().unwrap();

    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        state.zot_config.url, repository, reference
    );

    let response = client
        .delete(&manifest_url)
        .basic_auth(&state.zot_config.username, Some(&state.zot_config.password))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => HttpResponse::Ok().body("Suppression réussie!"),
        Ok(resp) => {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                HttpResponse::NotFound().body("Composant non trouvé")
            } else {
                HttpResponse::BadRequest().body(format!("Erreur suppression: {}", resp.status()))
            }
        }
        Err(e) => HttpResponse::InternalServerError().body(format!("Erreur: {}", e)),
    }
}
