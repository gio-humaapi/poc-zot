use actix_web::{web, App, HttpServer};
use poc::{
    controllers::{
        delete_component::delete_component, get_component::get_component,
        push_component::push_component, update_component::update_component,
    },
    entities,
};
use reqwest::Client;
use std::sync::Mutex;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Configuration de Zot
    let zot_config = entities::ZotConfig {
        url: "http://localhost:5000".to_string(),
        username: "user".to_string(),
        password: "password".to_string(),
    };

    let app_state = web::Data::new(entities::AppState {
        zot_config,
        client: Mutex::new(Client::new()),
    });

    println!("Serveur démarré sur http://localhost:8080");
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(push_component)
            .service(get_component)
            .service(update_component)
            .service(delete_component)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
