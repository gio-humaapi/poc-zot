use reqwest::Client;
use sha2::{Digest, Sha256};

pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub async fn upload_blob(
    client: &Client,
    url: &str,
    username: &str,
    password: &str,
    content: &Vec<u8>, // Changé en référence
    digest: &str,
) -> Result<(), String> {
    let response = client
        .put(url)
        .basic_auth(username, Some(password))
        .query(&[("digest", digest)])
        .body(content.clone()) // On clone ici si nécessaire pour reqwest
        .header("Content-Type", "application/octet-stream")
        .send()
        .await
        .map_err(|e| format!("Erreur upload: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Erreur statut: {}", response.status()));
    }
    Ok(())
}

pub async fn init_upload(
    client: &Client,
    base_url: &str,
    name: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let init_url = format!("{}/v2/{}/blobs/uploads/", base_url, name);
    let response = client
        .post(&init_url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|e| format!("Erreur init: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Erreur statut init: {}", response.status()));
    }

    let location = response
        .headers()
        .get("Location")
        .and_then(|loc| loc.to_str().ok())
        .unwrap_or_default()
        .to_string();

    Ok(if location.starts_with("http") {
        location
    } else {
        println!("Location: {}", location);
        format!("{}{}", base_url, location)
    })
}
