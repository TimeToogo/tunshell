FROM rust AS build

RUN mkdir /app/

COPY . /app/

WORKDIR /app/tunshell-server

RUN cargo build --release

FROM debian:buster-slim
RUN mkdir /app/

COPY --from=build /app/target/release/server /app/server
RUN chmod +x /app/server
COPY tunshell-server/static /app/static

WORKDIR /app

ENV STATIC_DIR /app/static

ENTRYPOINT [ "/app/server" ]