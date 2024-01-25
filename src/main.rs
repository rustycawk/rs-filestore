use actix_multipart::Multipart;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer};
use actix_web_prom::PrometheusMetricsBuilder;
use futures_util::StreamExt;
use openssl::symm::{Cipher, Crypter, Mode};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Result, Write};
use std::path::Path;

const KEY: &[u8] = b"\xb5M\xb1\x99\x96&\xdd\x9e\xe6:\xec\xbb\xc6\x81\xfd\xa5\xf7\x98\xc2 _\xc2R^]\xfc~M\xdbx\xfe\xb8";
const IV: &[u8] = b"cQ\x11\xf7&\xed\x83>\xcd&\xf4shz,x";
const STORAGE_PATH: &str = "storage/";
const BASE_URL: &str = "http://localhost:8080/";

fn encrypt(data: &[u8]) -> Vec<u8> {
    let cipher = Cipher::aes_256_cbc();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, KEY, Some(IV)).unwrap();
    let mut ciphertext = vec![0; data.len() + cipher.block_size()];
    let count = crypter.update(data, &mut ciphertext).unwrap();
    let rest = crypter.finalize(&mut ciphertext[count..]).unwrap();
    ciphertext.truncate(count + rest);
    ciphertext
}

fn decrypt(data: &[u8]) -> Vec<u8> {
    let cipher = Cipher::aes_256_cbc();
    let mut crypter = Crypter::new(cipher, Mode::Decrypt, KEY, Some(IV)).unwrap();
    let mut plaintext = vec![0; data.len() + cipher.block_size()];
    let count = crypter.update(data, &mut plaintext).unwrap();
    let rest = crypter.finalize(&mut plaintext[count..]).unwrap();
    plaintext.truncate(count + rest);
    plaintext
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
        let encrypted_buffer = encrypt(&buffer);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(STORAGE_PATH).join(&filename));
        file.unwrap()
            .write_all(&encrypted_buffer)
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
    let decrypted_buffer = decrypt(&buffer);
    Ok(HttpResponse::Ok().body(decrypted_buffer))
}

#[actix_web::main]
async fn main() -> Result<()> {
    if !Path::new(STORAGE_PATH).exists() {
        std::fs::create_dir(STORAGE_PATH).unwrap();
    }

    let mut labels = HashMap::new();
    labels.insert("label1".to_string(), "value1".to_string());
    let prometheus = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .const_labels(labels)
        .build()
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(prometheus.clone())
            .app_data(web::JsonConfig::default())
            .service(upload)
            .service(get)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await?;
    Ok(())
}
