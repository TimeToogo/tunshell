use crate::Config;
use anyhow::Result;
use std::{
    cmp,
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
    sync::{Arc, Mutex}
};
use tokio::io::{AsyncRead, AsyncWrite};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};
use log::*;
use futures::future::poll_fn;

pub struct ServerStream {
    state: Arc<Mutex<State>>
}

struct State {
    con_state: ConnectionState,

    recv_buff: Vec<u8>,
    recv_wakers: Vec<Waker>,

    send_buff: Vec<u8>,
    send_wakers: Vec<Waker>,

    close_wakers: Vec<Waker>,
}

impl State {
    fn new() -> Self {
        Self {
            con_state: ConnectionState::Connecting,
            recv_buff: vec![],
            recv_wakers: vec![],
            send_buff: vec![],
            send_wakers: vec![],
            close_wakers: vec![],
        }
    }

    fn wake_recv(&mut self) {
        for waker in self.recv_wakers.drain(..) {
            waker.wake();
        }
    }

    fn wake_send(&mut self) {
        for waker in self.send_wakers.drain(..) {
            waker.wake();
        }
    }

    fn wake_close(&mut self) {
        for waker in self.close_wakers.drain(..) {
            waker.wake();
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
enum ConnectionState {
    Connecting,
    Open,
    Closing,
    Closed
}

impl ServerStream {
    pub async fn connect(config: &Config) -> Result<Self> {
        let state = State::new();
        let state = Arc::new(Mutex::new(state));

        let url = format!("wss://{}:{}", config.relay_host(), config.relay_port());
        let ws = WebSocket::new(&url).unwrap();

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        
        // Recv
        {
            let state = Arc::clone(&state);

            let callback = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&abuf);
                    let len = array.byte_length() as usize;
                    debug!("message event, received arraybuffer of {} bytes", len);
                    
                    let mut state = state.lock().unwrap();

                    state.recv_buff.extend_from_slice(&array.to_vec()[..]);
                    state.wake_recv();
                } else if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
                    warn!("message event, received blob: {:?}", blob);
                } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                    warn!("message event, received Text: {:?}", txt);
                } else {
                    warn!("message event, received Unknown: {:?}", e.data());
                }
            }) as Box<dyn FnMut(MessageEvent)>);

            ws.set_onmessage(Some(callback.as_ref().unchecked_ref()));
            callback.forget();
        }

        // Error
        {
            let callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
                error!("error event: {:?}", e);
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(callback.as_ref().unchecked_ref()));
            callback.forget();
        }

        // Closed
        {
            let state = Arc::clone(&state);
            let callback = Closure::wrap(Box::new(move |_e: ErrorEvent| {
                let mut state = state.lock().unwrap();
                state.con_state = ConnectionState::Closed;
                state.wake_close();
                state.wake_recv();
                state.wake_send();
                debug!("socket closed");
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onclose(Some(callback.as_ref().unchecked_ref()));
            callback.forget();
        }

        // Open 
        {
            let state = Arc::clone(&state);
            let callback = Closure::wrap(Box::new(move |_| {
                let mut state = state.lock().unwrap();
                state.con_state = ConnectionState::Open;
                state.wake_send();
                debug!("socket opened");
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onopen(Some(callback.as_ref().unchecked_ref()));
            callback.forget();
        }

        // Write loop
        {
            let state = Arc::clone(&state);
            let ws = ws.clone();
            spawn_local(async move {
                loop {
                    // Wait until data is written to stream
                    let data = poll_fn(|cx| {
                        let mut state = state.lock().unwrap();
                        
                        if state.send_buff.len() == 0 || state.con_state == ConnectionState::Connecting {
                            state.send_wakers.push(cx.waker().clone());
                            return Poll::Pending;
                        }

                        Poll::Ready(state.send_buff.drain(..).collect::<Vec<u8>>())
                    }).await;

                    {
                        let state = state.lock().unwrap();

                        if state.con_state == ConnectionState::Closing || state.con_state == ConnectionState::Closed {
                            break;
                        }
                    }

                    match ws.send_with_u8_array(&data) {
                        Ok(_) => debug!("successfully sent {} bytes", data.len()),
                        Err(err) => debug!("error sending message: {:?}", err),
                    }
                }
            });
        }

        // Close loop
        {
            let state = Arc::clone(&state);
            let ws = ws.clone();
            spawn_local(async move {
                // Wait until signalled to close
                poll_fn(|cx| {
                    let mut state = state.lock().unwrap();
                    
                    if state.con_state != ConnectionState::Closing {
                        state.close_wakers.push(cx.waker().clone());
                        return Poll::Pending;
                    }

                    Poll::Ready(())
                }).await;

                debug!("closing socket");
                
                match ws.close() {
                    Ok(_) => debug!("successfully closed websocket"),
                    Err(err) => debug!("error while closing websocket: {:?}", err),
                }

                let mut state = state.lock().unwrap();
                state.con_state = ConnectionState::Closed;
                state.wake_recv();
                state.wake_send();
            });
        }

        Ok(Self {
            state
        })
    }
}

impl AsyncRead for ServerStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.state.lock().unwrap();

        if state.recv_buff.len() == 0 {
            if state.con_state == ConnectionState::Closing || state.con_state == ConnectionState::Closed {
                return Poll::Ready(Ok(0));
            }

            state.recv_wakers.push(cx.waker().clone());
            return Poll::Pending;
        }

        let len = cmp::min(buf.len(), state.recv_buff.len());
        &buf[..len].copy_from_slice(&state.recv_buff[..len]);
        state.recv_buff.drain(..len);
        Poll::Ready(Ok(len))
    }
}

impl AsyncWrite for ServerStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut state = self.state.lock().unwrap();

        if state.con_state == ConnectionState::Closing || state.con_state == ConnectionState::Closed {
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        }

        state.send_buff.extend_from_slice(buf);
        state.wake_send();

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        debug!("shutting down websocket");
        let mut state = self.state.lock().unwrap();

        if state.con_state == ConnectionState::Closing || state.con_state == ConnectionState::Closed {
            state.close_wakers.push(cx.waker().clone());
            return Poll::Pending;
        }

        state.con_state = ConnectionState::Closing;
        state.wake_close();
        Poll::Ready(Ok(()))
    }
}

impl Drop for ServerStream {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();

        if state.con_state == ConnectionState::Closing || state.con_state == ConnectionState::Closed {
            return;
        }

        state.con_state = ConnectionState::Closing;
        state.wake_close();
    }
}