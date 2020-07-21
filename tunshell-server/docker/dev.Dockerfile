FROM rust

RUN cargo install cargo-watch

WORKDIR /app/tunshell-server

CMD [ "cargo", "watch", "-x", "test", "-x", "run" ]