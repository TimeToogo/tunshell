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

ENV TEMPDIR /tmp
COPY ./install-deps.sh /tmp/install-deps.sh
RUN /tmp/install-deps.sh

CMD ["bash", "/workspace/build/compile.sh", "/workspace/build/targets.ubuntu.json"]