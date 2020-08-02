FROM rust:alpine AS build

RUN apk add --no-cache musl-dev

RUN mkdir /app/

COPY . /app/

WORKDIR /app/tunshell-client

RUN cargo build --release

FROM alpine:latest
RUN mkdir /app/

COPY --from=build /app/target/release/client /app/client
RUN chmod +x /app/client

WORKDIR /app

ENTRYPOINT [ "/app/client" ]