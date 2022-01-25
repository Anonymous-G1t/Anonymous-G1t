FROM rust:slim-buster

ADD . /src
WORKDIR /src

RUN cargo build --release

ENTRYPOINT [ "cargo", "run", "--release" ]
