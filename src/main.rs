mod amf;
mod error;
mod server;
mod stream;
mod utils;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use error::{Error, Result};
use server::{RtmpMediaStream, RtmpServer};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| String::from("7122"))
        .parse::<u16>()
        .expect("Invalid port number");
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.map_err(Error::Io)?;
    println!("Running RTMP server on port {}", port);

    let media_streams = Arc::new(Mutex::new(HashMap::<String, RtmpMediaStream>::new()));
    loop {
        let (stream, _) = listener.accept().await.map_err(Error::Io)?;
        let stream = stream.into_std().map_err(Error::Io)?;
        stream.set_nonblocking(false).map_err(Error::Io)?;
        let m = Arc::clone(&media_streams);
        tokio::spawn(async move {
            let mut server = RtmpServer::new(stream, m);
            if let Err(e) = server.serve() {
                eprintln!("Error: {}", e);
            }
        });
    }
}
