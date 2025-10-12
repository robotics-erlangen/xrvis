FROM rust:1.90
RUN apt-get update && apt-get install protobuf-compiler libwayland-dev libudev-dev libasound2-dev mold mingw-w64 -y && apt-get clean && rustup target add x86_64-pc-windows-gnu
