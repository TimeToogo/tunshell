use std::{
    cmp,
    collections::HashMap,
    io,
    sync::{
        self,
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use anyhow::{Error, Result};
use futures::StreamExt;
use remote_pty_common::channel;

use crate::shell::proto::{RemotePtyDataPayload, RemotePtyEventPayload, ShellServerMessage};

use super::ShellStream;

use remote_pty_master::{server::listener::Listener, *};

const STDIN_FD: libc::c_int = 0;

struct TransportRx {
    rx: sync::mpsc::Receiver<Vec<u8>>,
    recv_buff: Vec<u8>,
}

impl TransportRx {
    fn new(rx: sync::mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            recv_buff: vec![],
        }
    }
}

impl io::Read for TransportRx {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.recv_buff.is_empty() {
            let buf = self.rx.recv().map_err(|_| io::Error::last_os_error())?;
            self.recv_buff.extend_from_slice(buf.as_slice());
        }

        let n = cmp::min(buf.len(), self.recv_buff.len());
        buf[..n].copy_from_slice(&self.recv_buff[..n]);
        self.recv_buff.drain(..n);
        Ok(n)
    }
}

struct TransportTx {
    tx: sync::mpsc::Sender<Vec<u8>>,
}

impl TransportTx {
    fn new(tx: sync::mpsc::Sender<Vec<u8>>) -> Self {
        Self { tx }
    }
}

impl io::Write for TransportTx {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::last_os_error())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct TransportChannel {
    rx: TransportRx,
    tx: TransportTx,
}

impl TransportChannel {
    fn new(rx: sync::mpsc::Receiver<Vec<u8>>, tx: sync::mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            rx: TransportRx::new(rx),
            tx: TransportTx::new(tx),
        }
    }

    fn pair() -> (Self, Self) {
        let (t1, r1) = sync::mpsc::channel();
        let (t2, r2) = sync::mpsc::channel();

        (Self::new(r1, t2), Self::new(r2, t1))
    }

    fn streamed_to(
        self,
        con_id: u32,
        tx: tokio::sync::mpsc::UnboundedSender<RemotePtyDataPayload>,
        terminated: Arc<AtomicBool>,
    ) -> TransportTx {
        let rx = self.rx;
        tokio::task::spawn_blocking(move || -> Result<()> {
            loop {
                let data = rx.rx.recv()?;

                if terminated.load(Ordering::SeqCst) {
                    return Ok(());
                }

                tx.send(RemotePtyDataPayload { con_id, data })?;
            }
        });

        self.tx
    }
}

impl Into<channel::RemoteChannel> for TransportChannel {
    fn into(self) -> channel::RemoteChannel {
        channel::RemoteChannel::new(channel::transport::rw::ReadWriteTransport::new(
            self.rx, self.tx,
        ))
    }
}

struct RxListener {
    rx: sync::mpsc::Receiver<TransportChannel>,
}

impl Listener for RxListener {
    fn accept(&mut self) -> io::Result<channel::RemoteChannel> {
        Ok(self
            .rx
            .recv()
            .map_err(|_| io::Error::last_os_error())?
            .into())
    }
}

struct ChannelDemuxer {
    state: Arc<Mutex<HashMap<u32, (TransportTx, Arc<AtomicBool>)>>>,
    chan_tx: sync::mpsc::Sender<TransportChannel>,
    read_tx: tokio::sync::mpsc::UnboundedSender<RemotePtyDataPayload>,
    read_rx: tokio::sync::mpsc::UnboundedReceiver<RemotePtyDataPayload>,
}

impl ChannelDemuxer {
    fn new() -> (Self, RxListener) {
        let (chan_tx, chan_rx) = sync::mpsc::channel();
        let chan_rx = RxListener { rx: chan_rx };

        let (read_tx, read_rx) = tokio::sync::mpsc::unbounded_channel();

        let demux = Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            chan_tx,
            read_rx,
            read_tx,
        };

        (demux, chan_rx)
    }

    fn handle(&mut self, msg: RemotePtyEventPayload) -> Result<()> {
        match msg {
            RemotePtyEventPayload::Connect(con_id) => self.handle_new_connection(con_id),
            RemotePtyEventPayload::Payload(data) => self.handle_data(data),
            RemotePtyEventPayload::Close(con_id) => self.handle_close_connection(con_id),
        }
    }

    fn state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<u32, (TransportTx, Arc<AtomicBool>)>>> {
        self.state
            .lock()
            .map_err(|_| Error::msg("failed to lock mutex"))
    }

    fn handle_new_connection(&self, con_id: u32) -> Result<(), Error> {
        let mut state = self.state()?;

        if state.contains_key(&con_id) {
            return Err(Error::msg("connection id taken"));
        }

        let (c1, c2) = TransportChannel::pair();
        let terminated = Arc::new(AtomicBool::new(false));

        let tx1 = c1.streamed_to(con_id, self.read_tx.clone(), Arc::clone(&terminated));
        state.insert(con_id, (tx1, terminated));
        self.chan_tx
            .send(c2)
            .map_err(|_| Error::msg("failed to send new connection message"))?;

        Ok(())
    }

    fn handle_data(&self, data: RemotePtyDataPayload) -> Result<(), Error> {
        let state = self.state()?;

        if !state.contains_key(&data.con_id) {
            return Err(Error::msg("connection id does not exist"));
        }

        state
            .get(&data.con_id)
            .unwrap()
            .0
            .tx
            .send(data.data.to_vec())?;

        Ok(())
    }

    fn handle_close_connection(&self, con_id: u32) -> Result<(), Error> {
        let mut state = self.state()?;

        if !state.contains_key(&con_id) {
            return Err(Error::msg("connection id does not exist"));
        }

        let (_, terminated) = state.remove(&con_id).unwrap();
        terminated.store(true, Ordering::SeqCst);

        Ok(())
    }

    async fn next(&mut self) -> Result<RemotePtyDataPayload> {
        self.read_rx
            .recv()
            .await
            .ok_or_else(|| Error::msg("failed to recv message"))
    }
}

pub(super) async fn start_remote_pty_master(mut stream: ShellStream) -> Result<u8> {
    let (mut demuxer, listener) = ChannelDemuxer::new();

    let mut server = tokio::task::spawn_blocking(move || {
        let ctx = context::Context::from_pair(STDIN_FD, STDIN_FD);
        server::Server::new(ctx, Box::new(listener))
            .start()
            .join()
            .map_err(|_| Error::msg("failed to join server"))?;
        Ok(0u8)
    });

    loop {
        tokio::select! {
            res = &mut server => return res.unwrap_or_else(|_| Err(Error::msg("failed join server"))),
            msg = stream.next() => match msg {
                Some(Ok(ShellServerMessage::RemotePtyEvent(msg))) => demuxer.handle(msg)?,
                Some(Ok(ShellServerMessage::Exited(code))) => return Ok(code),
                None => return Err(Error::msg("shell server stream finished unexpectedly")),
                _ => return Err(Error::msg("received unexpected message from shell server stream"))
            },
            msg = demuxer.next() => {
                stream.write(&crate::shell::proto::ShellClientMessage::RemotePtyData(msg?)).await?;
            }
        };
    }
}
