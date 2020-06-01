FROM ubuntu:latest

VOLUME [ "/workspace" ]

# Install deps
RUN apt update -y
RUN apt install -y \
    curl \
    wget \
    vim \
    jq \
    binutils \
    make \
    build-essential \
    gcc-arm-linux-gnueabihf \
    crossbuild-essential-armhf

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install openssl to /tmp/openssl
RUN cd /tmp && wget https://www.openssl.org/source/openssl-1.1.1.tar.gz \
    && mkdir openssl \
    && tar xzf openssl-1.1.1.tar.gz -C openssl --strip-components 1

# Install libsodium to /tmp/libsodium
RUN cd /tmp && wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.18-stable.tar.gz \
    && mkdir libsodium \
    && tar xzf libsodium-1.0.18-stable.tar.gz -C libsodium --strip-components 1

CMD ["bash", "/workspace/build/cross-compile.sh"]