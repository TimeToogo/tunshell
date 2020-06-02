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
    gcc-arm-linux-gnueabi \
    crossbuild-essential-armhf

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
RUN echo 'source $HOME/.cargo/env' >> $HOME/.bashrc

ENV TEMP_DIR /tmp
COPY ./install-deps.sh /tmp/install-deps.sh
RUN /tmp/install-deps.sh

CMD ["bash", "/workspace/build/cross-compile.sh"]