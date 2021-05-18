use std::collections::HashMap;
use std::io::{Read, Result, Write};
use std::net::TcpStream;

use super::amf;

struct ChunkBasicHeader {
    chunk_stream_id: u16,
    chunk_type: u8,
}

#[derive(Default, Clone, Debug)]
struct ChunkMessageHeader {
    timestamp: u32,
    message_length: usize,
    message_type_id: u8,
    message_stream_id: u32,
}

#[derive(Debug)]
struct Message {
    header: ChunkMessageHeader,
    message: Vec<u8>,
}

pub struct RtmpServer {
    channels: HashMap<u16, Message>,
    stream: TcpStream,
    prev_message_header: Option<ChunkMessageHeader>,
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

const MAX_CHUNK_SIZE: usize = 128;

impl RtmpServer {
    fn handle_handshake(&mut self) -> Result<()> {
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

    fn read_chunk_basic_header(&mut self) -> Result<ChunkBasicHeader> {
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
        const MESSAGE_HEADER_SIZE: [u8; 4] = [11, 7, 3, 0];
        let mut buffer = vec![0x0; MESSAGE_HEADER_SIZE[chunk_type as usize] as usize];
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

    fn read_message(&mut self) -> Result<Option<Message>> {
        let basic_header = self.read_chunk_basic_header()?;
        let message_header = self.read_chunk_message_header(basic_header.chunk_type)?;
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

    fn handle_control_message(&mut self, message: Message) -> Result<()> {
        let mut reader = amf::AmfByteReader::from(&message.message);
        match amf::decode_amf_message(&mut reader)? {
            amf::AmfObject::String(cmd) => {
                if cmd == "connect" {
                    eprintln!("here");
                    let transaction_id = amf::decode_amf_message_number(&mut reader, true)?;
                    assert_eq!(transaction_id, 1_f64);
                    let cmd_object = amf::decode_amf_message_object(&mut reader, true)?;
                    eprintln!("cmd_object = {:?}", cmd_object);
                    // let optional_user_argument = amf::decode_amf_message_object(&mut reader, true)?;
                    // eprintln!("optional_user_argument = {:?}", optional_user_argument);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_message(&mut self, message: Message) -> Result<()> {
        match message.header.message_type_id {
            20 => {
                // AMF-0 encoded control message.
                self.handle_control_message(message);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn serve(&mut self) -> Result<()> {
        self.handle_handshake()?;
        loop {
            match self.read_message()? {
                None => {}
                Some(message) => {
                    eprintln!("message = {:?}", message);
                    assert_eq!(message.message.len(), message.header.message_length);
                    self.handle_message(message)?;
                }
            }
        }
        Ok(())
    }

    pub fn new(mut stream: TcpStream) -> RtmpServer {
        RtmpServer {
            channels: HashMap::new(),
            stream,
            prev_message_header: None,
        }
    }
}
