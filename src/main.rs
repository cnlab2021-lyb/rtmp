#![feature(assert_matches)]
use std::net::TcpListener;

mod amf;
mod server;
mod stream;

use server::RtmpServer;

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7122")?;

    for stream in listener.incoming() {
        let mut server = RtmpServer::new(stream?);
        server.serve()?;
    }
    Ok(())
}
