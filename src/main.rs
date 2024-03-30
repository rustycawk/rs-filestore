use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{get, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result};
use actix_web_prom::PrometheusMetricsBuilder;
use futures_util::StreamExt;
use image::ImageOutputFormat;
use num_cpus;
use openssl::symm::{Cipher, Crypter, Mode};
use prometheus::{opts, IntGauge};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Write};
use std::path::Path;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref KEY: &'static [u8] = {
        let key_data = std::fs::read("key").expect("Could not read key");
        assert_eq!(key_data.len(), 32);
        Box::leak(key_data.into_boxed_slice())
    };
    static ref IV: &'static [u8] = {
        let iv_data = std::fs::read("iv").expect("Could not read iv");
        assert_eq!(iv_data.len(), 16);
        Box::leak(iv_data.into_boxed_slice())
    };
    // static ref STORAGE_PATH: &'static str =
    //     &std::env::var("STORAGE_PATH").unwrap_or("storage".into())[..];
}
const STORAGE_PATH: &str = "storage/";
const BASE_URL: &str = "https://fs.5dev.kz/";
const BIND_ADDRESS: &str = "0.0.0.0:8471";

fn encrypt(data: &[u8]) -> Vec<u8> {
    let cipher = Cipher::aes_256_cbc();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, &KEY, Some(&IV)).unwrap();
    let mut ciphertext = vec![0; data.len() + cipher.block_size()];
    let count = crypter.update(data, &mut ciphertext).unwrap();
    let rest = crypter.finalize(&mut ciphertext[count..]).unwrap();
    ciphertext.truncate(count + rest);
    ciphertext
}

fn decrypt(data: &[u8]) -> Vec<u8> {
    let cipher = Cipher::aes_256_cbc();
    let mut crypter = Crypter::new(cipher, Mode::Decrypt, &KEY, Some(&IV)).unwrap();
    let mut plaintext = vec![0; data.len() + cipher.block_size()];
    let count = crypter.update(data, &mut plaintext).unwrap();
    let rest = crypter.finalize(&mut plaintext[count..]).unwrap();
    plaintext.truncate(count + rest);
    plaintext
}

fn generate_filename() -> String {
    let filename = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    if Path::new(&STORAGE_PATH).join(&filename).exists() {
        return generate_filename();
    }
    filename
}

#[post("/upload")]
async fn upload(mut payload: Multipart) -> Result<HttpResponse, Error> {
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
        .open(Path::new(&STORAGE_PATH).join(&filename));
    file.unwrap()
        .write_all(&encrypted_buffer)
        .expect("Could not write file");
    let mut map = HashMap::new();
    map.insert("link", format!("{}{}", &BASE_URL, &filename));
    return Ok(HttpResponse::Created().json(&map));
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
            let mut resized_buffer = Vec::new();
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
            (Ok(width), Err(_)) => Some((width, 0)),
            (Err(_), Ok(height)) => Some((0, height)),
            _ => None,
        }
    } else {
        None
    }
}

fn update_file_count_and_size(file_count: &IntGauge, combined_size: &IntGauge) {
    file_count.set(std::fs::read_dir(STORAGE_PATH).unwrap().count() as i64);
    combined_size.set(
        std::fs::read_dir(STORAGE_PATH)
            .unwrap()
            .fold(0, |acc, entry| {
                acc + entry.unwrap().metadata().unwrap().len() as i64
            }),
    );
}

#[actix_web::main]
async fn main() -> Result<()> {
    if !Path::new(STORAGE_PATH).exists() {
        std::fs::create_dir(STORAGE_PATH).unwrap();
    }
    let prometheus = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    let file_count_opts = opts!("file_count", "number of files in the storage").namespace("api");
    let file_count = IntGauge::with_opts(file_count_opts).unwrap();
    prometheus
        .registry
        .register(Box::new(file_count.clone()))
        .unwrap();
    let combined_size_opts =
        opts!("combined_size", "combined size of all files in bytes").namespace("api");
    let combined_size = IntGauge::with_opts(combined_size_opts).unwrap();
    prometheus
        .registry
        .register(Box::new(combined_size.clone()))
        .unwrap();
    update_file_count_and_size(&file_count, &combined_size);
    HttpServer::new(move || {
        App::new()
            .wrap(prometheus.clone())
            .wrap(Cors::default().allow_any_origin().send_wildcard())
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
