use std::net::TcpListener;

mod amf;
mod error;
mod server;
mod stream;
mod utils;

use error::{Error, Result};
use server::RtmpServer;

fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7122").map_err(Error::Io)?;

    for stream in listener.incoming() {
        let mut server = RtmpServer::new(stream.map_err(Error::Io)?);
        server.serve()?;
    }
    Ok(())
}
