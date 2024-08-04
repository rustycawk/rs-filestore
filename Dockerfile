FROM rust:1.79
WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs
COPY . .
RUN cargo build --release
CMD ["./target/release/rs-filestore"]