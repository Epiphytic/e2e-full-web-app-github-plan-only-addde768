use actix_web::{web, App, HttpServer, middleware};
use rusqlite::Connection;
use std::sync::Mutex;

mod auth;
mod db;
mod handlers;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub jwt_private_key: Vec<u8>,
    pub jwt_public_key: Vec<u8>,
    pub jwt_kid: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Generate keys if they don't exist
    let certs_dir = std::path::Path::new("certs");
    if !certs_dir.exists() {
        log::info!("Generating JWT keys...");
        let status = std::process::Command::new("bash")
            .arg("generate-keys.sh")
            .status()
            .expect("Failed to run generate-keys.sh");
        if !status.success() {
            panic!("Key generation failed");
        }
    }

    let private_key = std::fs::read("certs/private.pem")
        .expect("Failed to read private key. Run generate-keys.sh first.");
    let public_key = std::fs::read("certs/public.pem")
        .expect("Failed to read public key. Run generate-keys.sh first.");

    let conn = Connection::open("data.db").expect("Failed to open SQLite database");
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();

    let data = web::Data::new(AppState {
        db: Mutex::new(conn),
        jwt_private_key: private_key,
        jwt_public_key: public_key,
        jwt_kid: "sqlite-editor-key-1".to_string(),
    });

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    log::info!("Starting server on {}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(middleware::Logger::default())
            // Well-known endpoint for public keys (JWKS)
            .route("/.well-known/jwks.json", web::get().to(handlers::jwks_endpoint))
            // Auth endpoints
            .route("/api/auth/login", web::post().to(handlers::login))
            // Protected API endpoints
            .route("/api/tables", web::get().to(handlers::list_tables))
            .route("/api/tables", web::post().to(handlers::create_table))
            .route("/api/tables/{name}", web::delete().to(handlers::drop_table))
            .route("/api/tables/{name}/columns", web::get().to(handlers::list_columns))
            .route("/api/tables/{name}/columns", web::post().to(handlers::add_column))
            .route("/api/tables/{name}/columns/{column}", web::delete().to(handlers::drop_column))
            .route("/api/tables/{name}/rows", web::get().to(handlers::list_rows))
            .route("/api/tables/{name}/rows", web::post().to(handlers::insert_row))
            .route("/api/tables/{name}/rows/{rowid}", web::delete().to(handlers::delete_row))
            // HTML pages
            .route("/", web::get().to(handlers::index_page))
            .route("/login", web::get().to(handlers::login_page))
            .route("/tables", web::get().to(handlers::tables_page))
            .route("/tables/{name}", web::get().to(handlers::table_detail_page))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
