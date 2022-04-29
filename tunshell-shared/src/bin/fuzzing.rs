fn main() {
    #[cfg(fuzzing)]
    {
        use afl::*;
        use anyhow::Result;
        use futures::executor::block_on_stream;
        use futures::io::Cursor;
        use tunshell_shared::{ClientMessage, MessageStream, ServerMessage};
        
        fuzz!(|data: &[u8]| {
            let cursor = Cursor::new(data.to_vec());
            let stream_client: MessageStream<ServerMessage, ClientMessage, Cursor<Vec<u8>>> =
                MessageStream::new(cursor.clone());
            let stream_server: MessageStream<ClientMessage, ServerMessage, Cursor<Vec<u8>>> =
                MessageStream::new(cursor);

            let _ = block_on_stream(stream_client).collect::<Vec<Result<ClientMessage>>>();
            let _ = block_on_stream(stream_server).collect::<Vec<Result<ServerMessage>>>();
        });
    }
}
