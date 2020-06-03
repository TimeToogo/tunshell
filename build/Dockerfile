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
    crossbuild-essential-armhf \
    gcc-mingw-w64-x86-64 \
    gcc-mingw-w64-i686

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc

ENV TEMPDIR /tmp
COPY ./install-deps.sh /tmp/install-deps.sh
RUN /tmp/install-deps.sh

CMD ["bash", "/workspace/build/cross-compile.sh", "/workspace/build/targets.ubuntu.json"]