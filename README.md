# rs-filestore

This project contains a basic filestore implementation.

## Startup

Use `cargo run`. This will:

1. Create a configuration file `config.toml` with default values:

   ```toml
   [filestore]

   # Directory to store files. Please make sure that this directory exists and is writable.
   # Default: "storage/"
   path = "storage/" 

   # Expected base URL to the filestore host. This is used to output links to files.
   # Default: "http://host.example.com:8479"
   # (You'll need to change this!)
   base_url = "http://host.example.com:8479"

   # Address to listen on. Make sure to include the port and the IP address. Don't include the protocol ("http://", "https://").
   # Default: "0.0.0.0:8479"
   bind_address = "0.0.0.0:8479"
   ```

2. Create `key` and `iv` files with randomly filled values if they don't exist. These are used to encrypt and decrypt files.

3. Start the server on `0.0.0.0:8479`.

To just build the project, use `cargo build`. It will do the same as `cargo run` but without starting the server.

Please note that to start the server, you need to have a valid `config.toml` file, as well as `key` and `iv` files in the current working directory.

## Dependencies

Dependencies info is provided in [Cargo.toml](Cargo.toml).
