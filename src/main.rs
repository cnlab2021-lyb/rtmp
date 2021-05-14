use std::io::{Read, Result, Write};
use std::net::{TcpListener, TcpStream};

mod amf;

fn handle_handshake(stream: &mut TcpStream) -> Result<()> {
    let mut c0 = [0x0; 1];
    let s0 = [0x3; 1];
    stream.read_exact(&mut c0)?;
    assert_eq!(c0[0], 0x3);
    stream.write_all(&s0)?;
    const HANDSHAKE_SIZE: usize = 1536;
    let (mut c1, mut c2, mut s1, mut s2) = (
        [0x0; HANDSHAKE_SIZE],
        [0x0; HANDSHAKE_SIZE],
        [0x0; HANDSHAKE_SIZE],
        [0x0; HANDSHAKE_SIZE],
    );
    s1[8..].fill(0x11);
    stream.read_exact(&mut c1)?;
    stream.write_all(&s1)?;
    s2[8..].copy_from_slice(&c1[8..]);
    stream.write_all(&s2)?;
    stream.read_exact(&mut c2)?;
    assert!(c2[8..] == s1[8..]);
    Ok(())
}

fn read_bytes(stream: &mut TcpStream, nbytes: usize) -> Result<u64> {
    let mut buffer = vec![0; nbytes];
    stream.read_exact(&mut buffer)?;
    let mut result: u64 = 0;
    for byte in buffer {
        result = result << 8 | (byte as u64);
    }
    Ok(result)
}

#[derive(Debug, Default, Clone)]
struct MessageHeader {
    timestamp: u32,
    message_length: usize,
    message_type_id: u8,
    message_stream_id: u32,
}

#[derive(Default)]
struct MessageChunk {
    chunk_id: u16,
    chunk_type: u8,
    header: MessageHeader,
    message: Vec<u8>,
}

fn aggregate(buffer: &[u8]) -> u64 {
    buffer
        .iter()
        .fold(0_u64, |sum, &byte| sum << 8 | (byte as u64))
}

fn read_basic_header(stream: &mut TcpStream) -> Result<(u8, u16)> {
    let basic_header = read_bytes(stream, 1)? as u8;
    let (fmt, chunk_id) = (basic_header >> 6, basic_header & 0b111111);
    let chunk_id = match chunk_id {
        0x0 => 64 + read_bytes(stream, 1)? as u16,
        0x1 => 64 + read_bytes(stream, 2)? as u16,
        _ => chunk_id as u16,
    };
    Ok((fmt, chunk_id))
}

fn read_message_header(
    stream: &mut TcpStream,
    fmt: u8,
    prev_chunk: &Option<MessageChunk>,
) -> Result<MessageHeader> {
    const MESSAGE_HEADER_SIZE: [u8; 4] = [11, 7, 3, 0];
    let mut buffer = vec![0x0; MESSAGE_HEADER_SIZE[fmt as usize] as usize];
    stream.read_exact(&mut buffer)?;
    let mut message_header = match prev_chunk {
        None => MessageHeader::default(),
        Some(chunk) => chunk.header.clone(),
    };
    if fmt < 3 {
        message_header.timestamp = aggregate(&buffer[0..3]) as u32;
    }
    if fmt < 2 {
        message_header.message_length = aggregate(&buffer[3..6]) as usize;
        message_header.message_type_id = buffer[6];
    }
    if fmt == 0 {
        message_header.message_stream_id = aggregate(&buffer[7..11]) as u32;
    }
    // TODO: Handle type 3 header
    if fmt < 3 && message_header.timestamp >= 0xFFFFFF {
        assert_eq!(message_header.timestamp, 0xFFFFFF);
        let mut buffer = [0x0; 4];
        stream.read_exact(&mut buffer)?;
        message_header.timestamp = aggregate(&buffer) as u32;
    }
    Ok(message_header)
}

fn read_message_chunk(
    stream: &mut TcpStream,
    prev_chunk: &Option<MessageChunk>,
    max_chunk_size: usize,
) -> Result<MessageChunk> {
    let (chunk_type, chunk_id) = read_basic_header(stream)?;
    assert_eq!(prev_chunk.is_none(), chunk_type == 0x0);
    let message_header = read_message_header(stream, chunk_type, prev_chunk)?;
    eprintln!(
        "chunk_type = {}, chunk_id = {}, message_header = {:?}",
        chunk_type, chunk_id, message_header
    );
    let chunk_size = std::cmp::min(message_header.message_length, max_chunk_size);
    let mut chunk = MessageChunk {
        chunk_id,
        chunk_type,
        header: message_header,
        message: vec![0x0; chunk_size],
    };
    stream.read_exact(&mut chunk.message)?;
    chunk.header.message_length -= chunk_size;
    Ok(chunk)
}

fn handle_client(mut stream: TcpStream) -> Result<()> {
    handle_handshake(&mut stream)?;
    let mut prev_chunk = None;
    let mut max_chunk_size: usize = 128;
    loop {
        let chunk = read_message_chunk(&mut stream, &prev_chunk, max_chunk_size)?;
        match chunk.header.message_type_id {
            1 => {
                // Set chunk size.
                let mut buffer = [0x0; 4];
                stream.read_exact(&mut buffer)?;
                max_chunk_size = aggregate(&buffer) as usize;
            }
            _ => {}
        }
        prev_chunk = match chunk.header.message_length {
            0 => None,
            _ => Some(chunk),
        };
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7122")?;

    for stream in listener.incoming() {
        handle_client(stream?)?;
    }
    Ok(())
}
