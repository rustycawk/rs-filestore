use actix_multipart::Multipart;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::StreamExt;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Result, Write};
use std::path::Path;

const KEY: u8 = 21;
const STORAGE_PATH: &str = "storage/";
const BASE_URL: &str = "http://localhost:8080/";

fn encrypt(data: &mut [u8]) {
    for byte in data {
        *byte ^= KEY;
    }
}

fn decrypt(data: &mut [u8]) {
    encrypt(data);
}

fn generate_filename() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
}

#[post("/upload")]
async fn upload(mut payload: Multipart) -> Result<HttpResponse> {
    loop {
        let mut field = payload.next().await.unwrap().unwrap();
        let filename: String = generate_filename();
        let mut buffer = Vec::new();
        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            buffer.extend_from_slice(&data);
        }
        encrypt(&mut buffer);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(STORAGE_PATH).join(&filename));
        file.unwrap()
            .write_all(&buffer)
            .expect("Could not write file");
        let mut map = HashMap::new();
        map.insert("link", format!("{}{}", &BASE_URL, &filename));
        return Ok(HttpResponse::Created().json(&map));
    }
}

#[get("/{filename}")]
async fn get(req: HttpRequest) -> Result<HttpResponse> {
    let filename = req.match_info().get("filename").unwrap();
    let mut buffer = Vec::new();
    let mut file = OpenOptions::new()
        .read(true)
        .open(Path::new("storage/").join(filename))?;
    file.read_to_end(&mut buffer).expect("Could not read file");
    decrypt(&mut buffer);
    Ok(HttpResponse::Ok().body(buffer))
}

#[actix_web::main]
async fn main() -> Result<()> {
    if !Path::new(STORAGE_PATH).exists() {
        std::fs::create_dir(STORAGE_PATH).unwrap();
    }
    HttpServer::new(|| {
        App::new()
            .app_data(web::JsonConfig::default())
            .service(upload)
            .service(get)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
