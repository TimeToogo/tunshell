FROM rust:alpine

ENV RUSTFLAGS="--cfg alpine"

RUN apk add --no-cache musl-dev openssl-dev sqlite-dev
RUN cargo install cargo-watch

WORKDIR /app/tunshell-server

CMD [ "cargo", "watch", "-x", "test", "-x", "run" ]