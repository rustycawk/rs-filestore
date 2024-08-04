use std::fs::File;
use std::io::Write;
use std::path::Path;

fn generate_key() {
    let mut buf = [0; 32];
    for i in 0..32 {
        buf[i] = rand::random();
    }
    File::create("key")
        .and_then(|mut file| file.write_all(&buf))
        .expect("Could not create key");
}

fn generate_iv() {
    let mut buf = [0; 16];
    for i in 0..16 {
        buf[i] = rand::random();
    }
    File::create("iv")
        .and_then(|mut file| file.write_all(&buf))
        .expect("Could not create iv");
}

fn create_empty_config_file() {
    File::create("config.toml").expect("Could not create config.toml");
}

fn main() {
    if !Path::new("key").exists() {
        generate_key();
    }
    if !Path::new("iv").exists() {
        generate_iv();
    }
    if !Path::new("config.toml").exists() {
        create_empty_config_file();
    }
    println!("cargo:rerun-if-changed=config.toml");
}
