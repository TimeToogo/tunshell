FROM ubuntu:16.04

VOLUME [ "/workspace" ]

ENV TEMPDIR /tmp
COPY ./install-deps.sh /tmp/install-deps.sh
RUN /tmp/install-deps.sh

CMD ["bash", "/workspace/build/compile.sh", "/workspace/build/targets.ubuntu.json"]