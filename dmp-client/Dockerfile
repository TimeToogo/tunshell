FROM rustembedded/cross:aarch64-unknown-linux-gnu

ENV PKG_CONFIG_ALLOW_CROSS "true"
ENV SODIUM_STATIC "true"

RUN apt-get update && \
    apt-get install --assume-yes libssl-dev libsodium-dev