use actix_web::http::header::ContentType;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FileMeta {
    filename: String,
    sha256: String,
}

struct AppState {
    files: Mutex<HashMap<String, FileMeta>>,
    storage_dir: String,
}

async fn upload_file(mut payload: web::Payload, state: web::Data<AppState>) -> impl Responder {
    let mut sha256 = Sha256::new();
    let mut body = Vec::new();

    // Lock the Mutex to obtain mutable access to the HashMap
    let mut files = state.files.lock().unwrap();

    // Read the payload and compute SHA-256 hash
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.unwrap();
        sha256.update(&chunk);
        body.extend_from_slice(&chunk);
    }

    let sha256_hash = format!("{:x}", sha256.finalize());

    // Check if the file with the same hash already exists
    if let Some(existing_file) = files.get(&sha256_hash) {
        return HttpResponse::Ok().json(existing_file);
    }

    // Save the file to the storage directory
    let filename = format!("{}.dat", &sha256_hash);
    let file_path = Path::new(&state.storage_dir).join(&filename);
    fs::write(&file_path, &body).expect("Failed to write file");

    // Update the file metadata
    let file_meta = FileMeta {
        filename,
        sha256: sha256_hash.clone(),
    };
    files.insert(sha256_hash, file_meta.clone());

    HttpResponse::Created().json(file_meta)
}

async fn get_file(meta: web::Path<String>, state: web::Data<AppState>) -> impl Responder {
    let files = state.files.lock().unwrap();
    if let Some(file_meta) = files.get(&meta.into_inner()) {
        let file_path = Path::new(&state.storage_dir).join(&file_meta.filename);
        if let Ok(file_content) = fs::read(file_path) {
            HttpResponse::Ok()
                .insert_header(ContentType::octet_stream())
                .body(file_content)
        } else {
            HttpResponse::InternalServerError().finish()
        }
    } else {
        HttpResponse::NotFound().finish()
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    // Define storage directory
    let storage_dir = "storage";

    // Create the storage directory if it does not exist
    fs::create_dir_all(storage_dir).expect("Failed to create storage directory");

    // Create the AppState with an empty file map
    let app_state = web::Data::new(AppState {
        files: HashMap::new().into(),
        storage_dir: storage_dir.to_string(),
    });

    // Start the Actix HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/upload", web::post().to(upload_file))
            .route("/files/{file_hash}", web::get().to(get_file))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
