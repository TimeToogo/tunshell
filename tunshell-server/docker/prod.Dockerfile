FROM rust:alpine AS build

RUN apk add --no-cache musl-dev

RUN mkdir /app/

COPY . /app/

WORKDIR /app/tunshell-server

RUN cargo build --release

FROM alpine:latest
RUN mkdir /app/

COPY --from=build /app/target/release/server /app/server
RUN chmod +x /app/server
COPY tunshell-server/static /app/static

WORKDIR /app

ENV STATIC_DIR /app/static

ENTRYPOINT [ "/app/server" ]