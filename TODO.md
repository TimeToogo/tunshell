tunshell todo:

 - [x] clean up code / tests 
 - [x] finalise implementation for reliable-UDP protocol
 - [x] replace thrussh with custom protocol over ~~TLS~~ AES
 - [x] fix up "early eof" error on client by fixing the handling server-side
 - [x] re-write server in Rust
 - [ ] basic rust-only shell fallback for limited envs without pty 
 - [ ] windows support?
 - [ ] extend relay server to support websocket connections and implement client in-browser
 - [ ] docs / website / target init scripts for multiple langs

future scope:

 - [ ] integrate with https://github.com/uutils/coreutils for fallback shell
 - [ ] 