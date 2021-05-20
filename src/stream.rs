use std::collections::HashMap;
use std::io::{Read, Result, Write};
use std::net::TcpStream;

pub struct RtmpStream {
    channels: HashMap<u16, Message>,
    stream: TcpStream,
    prev_message_header: Option<ChunkMessageHeader>,
}

#[derive(Debug)]
pub struct ChunkBasicHeader {
    pub chunk_stream_id: u16,
    pub chunk_type: u8,
}

#[derive(Default, Clone, Debug)]
pub struct ChunkMessageHeader {
    pub timestamp: u32,
    pub message_length: usize,
    pub message_type_id: u8,
    pub message_stream_id: u32,
}

#[derive(Debug)]
pub struct Message {
    pub header: ChunkMessageHeader,
    pub message: Vec<u8>,
}

const MAX_CHUNK_SIZE: usize = 128;

impl RtmpStream {
    pub fn new(stream: TcpStream) -> Self {
        RtmpStream {
            channels: HashMap::new(),
            stream,
            prev_message_header: None,
        }
    }

    fn read_chunk_basic_header(&mut self) -> Result<ChunkBasicHeader> {
        eprintln!("read_chunk_basic_header");
        let header = read_bytes(&mut self.stream, 1)? as u8;
        let (chunk_type, chunk_stream_id) = (header >> 6, header & 0b111111);
        let chunk_stream_id = match chunk_stream_id {
            0x0 => 64 + read_bytes(&mut self.stream, 1)? as u16,
            0x1 => 64 + read_bytes(&mut self.stream, 2)? as u16,
            _ => chunk_stream_id as u16,
        };
        Ok(ChunkBasicHeader {
            chunk_stream_id,
            chunk_type,
        })
    }

    fn read_chunk_message_header(&mut self, chunk_type: u8) -> Result<ChunkMessageHeader> {
        eprintln!("read_chunk_message_header");
        const CHUNK_MESSAGE_HEADER_SIZE: [u8; 4] = [11, 7, 3, 0];
        let mut buffer = vec![0x0; CHUNK_MESSAGE_HEADER_SIZE[chunk_type as usize] as usize];
        self.stream.read_exact(&mut buffer)?;
        let mut message_header = match &self.prev_message_header {
            None => ChunkMessageHeader::default(),
            Some(chunk) => chunk.clone(),
        };
        let mut timestamp = 0;
        if chunk_type < 3 {
            timestamp = aggregate(&buffer[0..3]) as u32;
        }
        if chunk_type < 2 {
            message_header.message_length = aggregate(&buffer[3..6]) as usize;
            message_header.message_type_id = buffer[6];
        }
        if chunk_type == 0 {
            message_header.message_stream_id = aggregate(&buffer[7..11]) as u32;
        }
        // TODO: Handle type 3 header
        if chunk_type < 3 && timestamp >= 0xFFFFFF {
            assert_eq!(timestamp, 0xFFFFFF);
            let mut buffer = [0x0; 4];
            self.stream.read_exact(&mut buffer)?;
            timestamp = aggregate(&buffer) as u32;
        }
        if chunk_type == 0 {
            message_header.timestamp = timestamp;
        } else {
            message_header.timestamp += timestamp;
        }
        Ok(message_header)
    }

    pub fn read_message(&mut self) -> Result<Option<Message>> {
        eprintln!("read message");
        let basic_header = self.read_chunk_basic_header()?;
        let message_header = self.read_chunk_message_header(basic_header.chunk_type)?;
        eprintln!(
            "basic_header = {:?}, message_header = {:?}",
            basic_header, message_header
        );
        let mut result = None;
        match self.channels.get_mut(&basic_header.chunk_stream_id) {
            None => {
                let buffer_size = std::cmp::min(MAX_CHUNK_SIZE, message_header.message_length);
                let mut buffer = vec![0x0; buffer_size];
                self.stream.read_exact(&mut buffer)?;
                if buffer_size == message_header.message_length {
                    result = Some(Message {
                        header: message_header.clone(),
                        message: buffer,
                    })
                } else {
                    self.channels.insert(
                        basic_header.chunk_stream_id,
                        Message {
                            header: message_header.clone(),
                            message: buffer,
                        },
                    );
                }
            }
            Some(message) => {
                assert_eq!(basic_header.chunk_type, 3);
                let buffer_size = std::cmp::min(
                    MAX_CHUNK_SIZE,
                    message.header.message_length - message.message.len(),
                );
                let mut buffer = vec![0x0; buffer_size];
                self.stream.read_exact(&mut buffer)?;
                message.message.extend_from_slice(&buffer);
                if message.message.len() == message.header.message_length {
                    result = self.channels.remove(&basic_header.chunk_stream_id);
                }
            }
        }
        self.prev_message_header = Some(message_header);
        Ok(result)
    }

    pub fn handle_handshake(&mut self) -> Result<()> {
        let mut c0 = [0x0; 1];
        let s0 = [0x3; 1];
        self.stream.read_exact(&mut c0)?;
        assert_eq!(c0[0], 0x3);
        self.stream.write_all(&s0)?;
        const HANDSHAKE_SIZE: usize = 1536;
        let (mut c1, mut c2, mut s1, mut s2) = (
            [0x0; HANDSHAKE_SIZE],
            [0x0; HANDSHAKE_SIZE],
            [0x0; HANDSHAKE_SIZE],
            [0x0; HANDSHAKE_SIZE],
        );
        s1[8..].fill(0x11);
        self.stream.read_exact(&mut c1)?;
        self.stream.write_all(&s1)?;
        s2[8..].copy_from_slice(&c1[8..]);
        self.stream.write_all(&s2)?;
        self.stream.read_exact(&mut c2)?;
        assert!(c2[8..] == s1[8..]);
        Ok(())
    }

    fn send_chunk_basic_header(&mut self, header: ChunkBasicHeader) -> Result<()> {
        if header.chunk_stream_id < 64 {
            let byte = (header.chunk_stream_id as u8) | (header.chunk_type << 6);
            self.stream.write_all(&byte.to_be_bytes())?;
        } else if header.chunk_stream_id < 320 {
            let buffer = [
                header.chunk_type << 6 | 1,
                (header.chunk_stream_id - 64) as u8,
            ];
            self.stream.write_all(&buffer)?;
        } else {
            let buffer = [
                header.chunk_type << 6,
                ((header.chunk_stream_id - 64) >> 8) as u8,
                ((header.chunk_stream_id - 64) & 255) as u8,
            ];
            self.stream.write_all(&buffer)?;
        }
        Ok(())
    }

    fn send_chunk_message_header(
        &mut self,
        header: ChunkMessageHeader,
        chunk_type: u8,
    ) -> Result<()> {
        let mut buffer = Vec::new();
        if chunk_type < 3 {
            let timestamp = if header.timestamp >= 0xFFFFFF {
                0xFFFFFF
            } else {
                header.timestamp
            };
            buffer.extend_from_slice(&[
                ((timestamp >> 16) & 255) as u8,
                ((timestamp >> 8) & 255) as u8,
                (timestamp & 255) as u8,
            ]);
        }
        if chunk_type < 2 {
            buffer.extend_from_slice(&[
                ((header.message_length >> 16) & 255) as u8,
                ((header.message_length >> 8) & 255) as u8,
                (header.message_length & 255) as u8,
            ]);
            buffer.push(header.message_type_id);
        }
        if chunk_type == 0 {
            buffer.extend_from_slice(&header.message_stream_id.to_be_bytes());
        }
        if chunk_type < 3 && header.timestamp >= 0xFFFFFF {
            buffer.extend_from_slice(&header.timestamp.to_be_bytes());
        }
        self.stream.write_all(&buffer)?;
        Ok(())
    }

    pub fn send_message(
        &mut self,
        chunk_stream_id: u16,
        message_stream_id: u32,
        message_type_id: u8,
        message: &[u8],
    ) -> Result<()> {
        let mut ptr = 0;
        while ptr < message.len() {
            let size = std::cmp::min(MAX_CHUNK_SIZE, message.len() - ptr);
            let chunk_type: u8 = if ptr == 0 { 0 } else { 3 };
            self.send_chunk_basic_header(ChunkBasicHeader {
                chunk_stream_id,
                chunk_type,
            })?;
            self.send_chunk_message_header(
                ChunkMessageHeader {
                    timestamp: 0,
                    message_length: message.len(),
                    message_type_id,
                    message_stream_id,
                },
                chunk_type,
            )?;
            self.stream.write_all(&message[ptr..ptr + size])?;
            ptr += size;
        }
        Ok(())
    }
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

fn aggregate(buffer: &[u8]) -> u64 {
    buffer
        .iter()
        .fold(0_u64, |sum, &byte| sum << 8 | (byte as u64))
}
