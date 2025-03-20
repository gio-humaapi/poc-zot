use actix_web::{App, HttpServer};
use zot::controllers::{get_component, push_component, update_component};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Serveur démarré sur http://localhost:8080");
    HttpServer::new(|| App::new().service(push_component).service(get_component).service(update_component))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
