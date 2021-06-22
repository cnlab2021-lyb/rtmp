use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

mod amf;
mod constant;
mod error;
mod server;
mod stream;
mod utils;

use error::{Error, Result};
use server::{RtmpMediaStream, RtmpServer};

fn main() -> Result<()> {
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| String::from("7122"))
        .parse::<u16>()
        .expect("Invalid port number");
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).map_err(Error::Io)?;
    println!("Running RTMP server on port {}", port);

    let media_streams = Arc::new(Mutex::new(HashMap::<String, RtmpMediaStream>::new()));

    for stream in listener.incoming() {
        let m = Arc::clone(&media_streams);
        let stream = stream.map_err(Error::Io)?;
        thread::spawn(move || {
            let mut server = RtmpServer::new(stream, m);
            if let Err(e) = server.serve() {
                eprintln!("Error: {}", e);
            }
        });
    }
    Ok(())
}
