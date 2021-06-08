use std::collections::HashMap;
use std::io::{self, Write};
use std::net::TcpStream;

use super::error::{Error, Result};
use super::utils::{aggregate, read_buffer, read_buffer_sized, read_numeric, read_u32};

pub struct RtmpStream {
    channels: HashMap<u16, Message>,
    prev_message_header: HashMap<u16, ChunkMessageHeader>,
    stream: TcpStream,
    pub max_chunk_size_read: usize,
    pub max_chunk_size_write: usize,
}

#[derive(Debug)]
struct ChunkBasicHeader {
    chunk_stream_id: u16,
    chunk_type: u8,
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

impl RtmpStream {
    pub fn new(stream: TcpStream) -> Self {
        RtmpStream {
            channels: HashMap::new(),
            prev_message_header: HashMap::new(),
            stream,
            max_chunk_size_read: 128,
            max_chunk_size_write: 128,
        }
    }

    fn read_chunk_basic_header(&mut self) -> io::Result<ChunkBasicHeader> {
        let header = read_numeric::<u8, _>(&mut self.stream, 1)?;
        let (chunk_type, chunk_stream_id) = (header >> 6, header & 0b111111);
        let chunk_stream_id = match chunk_stream_id {
            0x0 => 64 + read_numeric::<u16, _>(&mut self.stream, 1)?,
            0x1 => 64 + read_numeric::<u16, _>(&mut self.stream, 2)?,
            _ => chunk_stream_id as u16,
        };
        Ok(ChunkBasicHeader {
            chunk_stream_id,
            chunk_type,
        })
    }

    fn read_chunk_message_header(
        &mut self,
        basic_header: &ChunkBasicHeader,
    ) -> Result<ChunkMessageHeader> {
        const CHUNK_MESSAGE_HEADER_SIZE: [usize; 4] = [11, 7, 3, 0];
        let mut message_header = match self.prev_message_header.get(&basic_header.chunk_stream_id) {
            None => ChunkMessageHeader::default(),
            Some(h) => h.clone(),
        };
        let chunk_type = basic_header.chunk_type;
        if chunk_type == 3 {
            return Ok(message_header);
        }
        let buffer = read_buffer(
            &mut self.stream,
            CHUNK_MESSAGE_HEADER_SIZE[chunk_type as usize],
        )
        .map_err(Error::Io)?;
        if chunk_type < 2 {
            message_header.message_length = aggregate::<usize>(&buffer[3..6], false);
            message_header.message_type_id = buffer[6];
        }
        if chunk_type == 0 {
            message_header.message_stream_id = aggregate::<u32>(&buffer[7..11], true);
        }
        if chunk_type < 3 {
            let timestamp = aggregate::<u32>(&buffer[0..3], false);
            let timestamp = match timestamp {
                0..=0xFFFFFE => timestamp,
                0xFFFFFF => read_u32(&mut self.stream).map_err(Error::Io)?,
                _ => {
                    return Err(Error::InvalidTimestamp);
                }
            };
            if chunk_type == 0 {
                message_header.timestamp = timestamp;
            } else {
                message_header.timestamp += timestamp;
            }
        }
        Ok(message_header)
    }

    pub fn read_message(&mut self) -> Result<Option<Message>> {
        let basic_header = self.read_chunk_basic_header().map_err(Error::Io)?;
        eprintln!("basic_header = {:?}", basic_header);
        let message_header = self.read_chunk_message_header(&basic_header)?;
        eprintln!("message_header = {:?}", message_header);
        let result = match self.channels.get_mut(&basic_header.chunk_stream_id) {
            None => {
                let buffer_size =
                    std::cmp::min(self.max_chunk_size_read, message_header.message_length);
                let buffer = read_buffer(&mut self.stream, buffer_size).map_err(Error::Io)?;
                if buffer_size == message_header.message_length {
                    Some(Message {
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
                    None
                }
            }
            Some(message) => {
                assert_eq!(basic_header.chunk_type, 3);
                let buffer_size = std::cmp::min(
                    self.max_chunk_size_read,
                    message.header.message_length - message.message.len(),
                );
                message.message.extend_from_slice(
                    &read_buffer(&mut self.stream, buffer_size).map_err(Error::Io)?,
                );
                if message.message.len() == message.header.message_length {
                    self.channels.remove(&basic_header.chunk_stream_id)
                } else {
                    None
                }
            }
        };
        self.prev_message_header
            .insert(basic_header.chunk_stream_id, message_header);
        Ok(result)
    }

    pub fn handle_handshake(&mut self) -> Result<()> {
        let c0 = read_buffer_sized::<_, 1>(&mut self.stream).map_err(Error::Io)?;
        if c0[0] != 0x3 {
            return Err(Error::HandshakeCorrupted);
        }
        let s0 = [0x3; 1];
        self.stream.write_all(&s0).map_err(Error::Io)?;
        const HANDSHAKE_SIZE: usize = 1536;
        let c1 = read_buffer_sized::<_, HANDSHAKE_SIZE>(&mut self.stream).map_err(Error::Io)?;
        // Send a buffer consisting of random bytes.
        let s1: Vec<_> = (0..HANDSHAKE_SIZE)
            .map(|i| if i < 8 { 0 } else { rand::random::<u8>() })
            .collect();
        self.stream.write_all(&s1).map_err(Error::Io)?;
        let s2 = c1;
        self.stream.write_all(&s2).map_err(Error::Io)?;
        let c2 = read_buffer_sized::<_, HANDSHAKE_SIZE>(&mut self.stream).map_err(Error::Io)?;
        if c2[8..] == s1[8..] {
            Ok(())
        } else {
            Err(Error::HandshakeCorrupted)
        }
    }

    fn send_chunk_basic_header(&mut self, header: ChunkBasicHeader) -> Result<()> {
        (if header.chunk_stream_id < 64 {
            let byte = (header.chunk_stream_id as u8) | (header.chunk_type << 6);
            self.stream.write_all(&[byte])
        } else if header.chunk_stream_id < 320 {
            self.stream.write_all(&[
                header.chunk_type << 6 | 1,
                (header.chunk_stream_id - 64) as u8,
            ])
        } else {
            self.stream.write_all(&[
                header.chunk_type << 6,
                ((header.chunk_stream_id - 64) >> 8) as u8,
                ((header.chunk_stream_id - 64) & 255) as u8,
            ])
        })
        .map_err(Error::Io)?;
        Ok(())
    }

    fn send_chunk_message_header(
        &mut self,
        header: ChunkMessageHeader,
        chunk_type: u8,
    ) -> Result<()> {
        // The maximum size of header is 11 bytes.
        let mut buffer = Vec::with_capacity(11);
        if chunk_type < 3 {
            let timestamp = if header.timestamp >= 0xFFFFFF {
                0xFFFFFF
            } else {
                header.timestamp
            };
            buffer.extend_from_slice(&timestamp.to_be_bytes()[1..]);
        }
        if chunk_type < 2 {
            buffer.extend_from_slice(
                &header.message_length.to_be_bytes()[std::mem::size_of::<usize>() - 3..],
            );
            buffer.push(header.message_type_id);
        }
        if chunk_type == 0 {
            buffer.extend_from_slice(&header.message_stream_id.to_le_bytes());
        }
        if chunk_type < 3 && header.timestamp >= 0xFFFFFF {
            buffer.extend_from_slice(&header.timestamp.to_be_bytes());
        }
        self.stream.write_all(&buffer).map_err(Error::Io)?;
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
            let size = std::cmp::min(self.max_chunk_size_write, message.len() - ptr);
            let chunk_type = if ptr == 0 { 0 } else { 3 };
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
            self.stream
                .write_all(&message[ptr..ptr + size])
                .map_err(Error::Io)?;
            ptr += size;
        }
        Ok(())
    }
}
