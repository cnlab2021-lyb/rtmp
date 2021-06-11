use std::net::TcpListener;
use std::thread;

mod amf;
mod error;
mod server;
mod stream;
mod utils;

use error::{Error, Result};
use server::RtmpServer;

fn main() -> Result<()> {
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| String::from("7122"))
        .parse::<u16>()
        .expect("Invalid port number");
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).map_err(Error::Io)?;
    println!("Running RTMP server on port {}", port);

    for stream in listener.incoming() {
        thread::spawn(|| -> Result<()> {
            let mut server = RtmpServer::new(stream.map_err(Error::Io)?);
            server.serve()
        });
    }
    Ok(())
}
