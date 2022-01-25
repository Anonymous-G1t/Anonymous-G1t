FROM rust:slim-buster

RUN cargo build --release

ENTRYPOINT [ "/usr/bin/cargo", "run", "--release" ]

