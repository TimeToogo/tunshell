FROM rust:alpine

RUN apk add --no-cache musl-dev
RUN cargo install cargo-watch

WORKDIR /app/tunshell-server

CMD [ "cargo", "watch", "-x", "test", "-x", "run" ]