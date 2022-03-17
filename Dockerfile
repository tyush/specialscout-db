FROM rust:1.59-buster

# create an empty shell project
RUN USER=root cargo new --bin specialscout-db
WORKDIR /specialscout-db

COPY . .

RUN rustup install nightly

RUN cargo +nightly build --release

EXPOSE 8080

CMD ["target/release/specialscout-db"]