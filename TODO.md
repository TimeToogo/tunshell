tunshell todo:

 - [x] clean up code / tests 
 - [x] finalise implementation for reliable-UDP protocol
 - [x] replace thrussh with custom protocol over ~~TLS~~ AES
 - [x] fix up "early eof" error on client by fixing the handling server-side
 - [x] re-write server in Rust
 - [x] windows support?
 - [x] security enhancements
    - [x] website to generate PSK for AEAD stream for relay and direct connections
    - [x] script templates to be moved to S3
    - [x] move logic to decide on target/client host to client binary
 - [ ] basic rust-only shell fallback for limited envs without pty 
 - [ ] extend relay server to support websocket connections and implement client in-browser
 - [ ] init scripts for multiple langs
    - [x] sh/bash
    - [x] cmd/powershell
    - [x] node
    - [ ] python
    - [ ] C#
    - [ ] java
 - [ ] migrate aws account to OU
 - [ ] docs / website

future scope:

 - [ ] integrate with https://github.com/uutils/coreutils for enhanced fallback shell
 - [ ] 