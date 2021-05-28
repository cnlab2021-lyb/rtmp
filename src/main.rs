#![feature(assert_matches)]
use std::net::TcpListener;

mod amf;
mod error;
mod server;
mod stream;
mod utils;

use error::{Result, RtmpError};
use server::RtmpServer;

fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7122").map_err(RtmpError::Io)?;

    for stream in listener.incoming() {
        let mut server = RtmpServer::new(stream.map_err(RtmpError::Io)?);
        server.serve()?;
    }
    Ok(())
}
