use actix_multipart::Multipart;
use actix_web::{get, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result};
use actix_web_prom::PrometheusMetricsBuilder;
use futures_util::StreamExt;
use image::ImageOutputFormat;
use num_cpus;
use openssl::symm::{Cipher, Crypter, Mode};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Write};
use std::path::Path;

const KEY: &[u8] = b"\xb5M\xb1\x99\x96&\xdd\x9e\xe6:\xec\xbb\xc6\x81\xfd\xa5\xf7\x98\xc2 _\xc2R^]\xfc~M\xdbx\xfe\xb8";
const IV: &[u8] = b"cQ\x11\xf7&\xed\x83>\xcd&\xf4shz,x";
const STORAGE_PATH: &str = "storage/";
const BASE_URL: &str = "http://localhost:8080/";
const BIND_ADDRESS: &str = "0.0.0.0:8080";

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
async fn get(req: HttpRequest) -> Result<HttpResponse, Error> {
    let filename = req.match_info().get("filename").unwrap();
    let mut buffer = Vec::new();
    let mut file = OpenOptions::new()
        .read(true)
        .open(Path::new(STORAGE_PATH).join(filename))?;
    file.read_to_end(&mut buffer).expect("Could not read file");
    let decrypted_buffer = decrypt(&buffer);
    let params: HashMap<_, _> = req
        .query_string()
        .split('&')
        .filter_map(|s| {
            let mut iter = s.split('=');
            iter.next()
                .and_then(|key| iter.next().map(|value| (key, value)))
        })
        .collect();
    if let Some(&resize) = params.get("resize") {
        if let Some((mut width, mut height)) = parse_resize_param(resize) {
            let image = image::load_from_memory(&decrypted_buffer)
                .expect("Failed to load image from buffer");
            if width == 0 || height == 0 {
                let aspect_ratio = image.width() as f32 / image.height() as f32;
                if width == 0 {
                    width = (height as f32 * aspect_ratio) as u32;
                } else {
                    height = (width as f32 / aspect_ratio) as u32;
                }
            }
            let resized_image =
                image.resize_exact(width, height, image::imageops::FilterType::Nearest);
            let mut resized_buffer = Vec::new(); // Create a new buffer for the resized image
            resized_image
                .write_to(
                    &mut Cursor::new(&mut resized_buffer),
                    ImageOutputFormat::Jpeg(90),
                )
                .expect("Failed to write resized image to buffer");
            return Ok(HttpResponse::Ok()
                .content_type("image/jpeg")
                .body(resized_buffer));
        }
    }
    Ok(HttpResponse::Ok().body(decrypted_buffer))
}

fn parse_resize_param(param: &str) -> Option<(u32, u32)> {
    let sizes: Vec<&str> = param.split('x').collect();
    if sizes.len() == 2 {
        match (sizes[0].parse::<u32>(), sizes[1].parse::<u32>()) {
            (Ok(width), Ok(height)) => Some((width, height)),
            (Ok(width), Err(_)) => Some((width, 0)), // Width specified, height is auto
            (Err(_), Ok(height)) => Some((0, height)), // Height specified, width is auto
            _ => None,
        }
    } else {
        None
    }
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
    .bind(BIND_ADDRESS)?
    .workers(num_cpus::get())
    .run()
    .await?;
    Ok(())
}
