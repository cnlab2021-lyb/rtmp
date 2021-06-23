use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::error::{Error, Result};
use crate::utils::{aggregate, read_buffer, read_buffer_sized, read_numeric, read_u32};

pub trait TryClone: Sized {
    fn try_clone(&self) -> io::Result<Self>;
}

impl TryClone for TcpStream {
    #[inline]
    fn try_clone(&self) -> io::Result<TcpStream> {
        self.try_clone()
    }
}

#[derive(Debug)]
pub struct RtmpMessageStreamImpl<S: TryClone + Read + Write + AsRawFd> {
    pub channels: HashMap<u16, Message>,
    prev_message_header: HashMap<u16, (ChunkMessageHeader, u8)>,
    stream: S,
    pub from_fd: RawFd,
    pub max_chunk_size_read: usize,
    pub max_chunk_size_write: usize,
}

pub type RtmpMessageStream = RtmpMessageStreamImpl<TcpStream>;

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

    timestamp_delta: u32,
}

#[derive(Debug)]
pub struct Message {
    pub header: ChunkMessageHeader,
    pub message: Vec<u8>,
}

impl Message {
    fn new(header: ChunkMessageHeader) -> Self {
        Message {
            header,
            message: Vec::new(),
        }
    }
}

impl<S: TryClone + Read + Write + AsRawFd> RtmpMessageStreamImpl<S> {
    pub fn new(stream: S) -> Self {
        let from_fd = stream.as_raw_fd();
        Self {
            channels: HashMap::new(),
            prev_message_header: HashMap::new(),
            stream,
            from_fd,
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
        let (mut message_header, prev_chunk_type) =
            if let Some(h) = self.prev_message_header.get(&basic_header.chunk_stream_id) {
                h.clone()
            } else {
                (ChunkMessageHeader::default(), 0)
            };

        if basic_header.chunk_type == 3 {
            if prev_chunk_type == 0 {
                message_header.timestamp_delta = message_header.timestamp;
            }
            message_header.timestamp =
                (message_header.timestamp + message_header.timestamp_delta) % 0xFFFFFF;
            return Ok(message_header);
        }
        const CHUNK_MESSAGE_HEADER_SIZE: [usize; 4] = [11, 7, 3, 0];
        let buffer = read_buffer(
            &mut self.stream,
            CHUNK_MESSAGE_HEADER_SIZE[basic_header.chunk_type as usize],
        )
        .map_err(Error::Io)?;
        if basic_header.chunk_type < 2 {
            message_header.message_length = aggregate::<usize>(&buffer[3..6], false);
            message_header.message_type_id = buffer[6];
        }
        if basic_header.chunk_type == 0 {
            message_header.message_stream_id = aggregate::<u32>(&buffer[7..11], true);
        }
        let timestamp_or_delta = aggregate::<u32>(&buffer[0..3], false);
        let timestamp_or_delta = match timestamp_or_delta {
            0..=0xFFFFFE => timestamp_or_delta,
            0xFFFFFF => read_u32(&mut self.stream).map_err(Error::Io)?,
            _ => {
                return Err(Error::InvalidTimestamp);
            }
        };
        if basic_header.chunk_type == 0 {
            message_header.timestamp = timestamp_or_delta;
            message_header.timestamp_delta = 0;
        } else {
            message_header.timestamp_delta = timestamp_or_delta;
            message_header.timestamp =
                (message_header.timestamp + message_header.timestamp_delta) % 0xFFFFFF;
        }
        Ok(message_header)
    }

    pub fn read_message(&mut self) -> Result<Option<Message>> {
        let basic_header = self.read_chunk_basic_header().map_err(Error::Io)?;
        let message_header = self.read_chunk_message_header(&basic_header)?;
        let is_first_chunk = !self.channels.contains_key(&basic_header.chunk_stream_id);
        let msg = self
            .channels
            .entry(basic_header.chunk_stream_id)
            .or_insert_with(|| Message::new(message_header.clone()));
        let buffer_size = std::cmp::min(
            self.max_chunk_size_read,
            msg.header.message_length - msg.message.len(),
        );
        msg.message
            .extend_from_slice(&read_buffer(&mut self.stream, buffer_size).map_err(Error::Io)?);
        let result = if msg.message.len() == msg.header.message_length {
            self.channels.remove(&basic_header.chunk_stream_id)
        } else {
            None
        };

        if is_first_chunk {
            self.prev_message_header.insert(
                basic_header.chunk_stream_id,
                (message_header, basic_header.chunk_type),
            );
        }
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
        if header.chunk_stream_id < 64 {
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
        }
        .map_err(Error::Io)?;
        Ok(())
    }

    fn send_chunk_message_header(
        &mut self,
        header: ChunkMessageHeader,
        chunk_type: u8,
    ) -> Result<()> {
        if chunk_type == 3 {
            return Ok(());
        }
        // The maximum size of header is 11 bytes.
        let mut buffer = Vec::with_capacity(11);
        let timestamp_or_delta = if chunk_type == 0 {
            header.timestamp
        } else {
            header.timestamp_delta
        };
        let timestamp_or_delta_non_extended = if timestamp_or_delta >= 0xFFFFFF {
            0xFFFFFF
        } else {
            timestamp_or_delta
        };
        buffer.extend_from_slice(&timestamp_or_delta_non_extended.to_be_bytes()[1..]);
        if chunk_type < 2 {
            buffer.extend_from_slice(
                &header.message_length.to_be_bytes()[std::mem::size_of::<usize>() - 3..],
            );
            buffer.push(header.message_type_id);
        }
        if chunk_type == 0 {
            buffer.extend_from_slice(&header.message_stream_id.to_le_bytes());
        }
        if chunk_type < 3 && timestamp_or_delta >= 0xFFFFFF {
            buffer.extend_from_slice(&timestamp_or_delta.to_be_bytes());
        }
        self.stream.write_all(&buffer).map_err(Error::Io)?;
        Ok(())
    }

    pub fn send_message(
        &mut self,
        chunk_stream_id: u16,
        message_stream_id: u32,
        timestamp: u32,
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
                    timestamp,
                    message_length: message.len(),
                    message_type_id,
                    message_stream_id,
                    timestamp_delta: 0,
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

    pub fn decouple(&self) -> Self {
        Self {
            channels: HashMap::new(),
            prev_message_header: HashMap::new(),
            stream: self.stream.try_clone().expect("Failed to clone"),
            from_fd: self.from_fd,
            max_chunk_size_read: self.max_chunk_size_read,
            max_chunk_size_write: self.max_chunk_size_write,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockTcpStream {
        cursor: io::Cursor<Vec<u8>>,
        buffer: Vec<u8>,
    }

    impl Read for MockTcpStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.cursor.read(buf)
        }
    }

    impl Write for MockTcpStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl TryClone for MockTcpStream {
        fn try_clone(&self) -> io::Result<Self> {
            Ok(Self::default())
        }
    }

    impl AsRawFd for MockTcpStream {
        fn as_raw_fd(&self) -> RawFd {
            0
        }
    }

    impl MockTcpStream {
        fn consume_buffer(&mut self) {
            self.cursor = io::Cursor::new(self.buffer.drain(..).collect());
        }
    }

    type MockRtmpMessageStream = RtmpMessageStreamImpl<MockTcpStream>;

    #[test]
    fn test_basic_header() {
        let mock = MockTcpStream {
            cursor: io::Cursor::new(vec![0x3]),
            buffer: Vec::new(),
        };
        let mut stream = MockRtmpMessageStream::new(mock);
        let basic_header = stream.read_chunk_basic_header().unwrap();
        assert_eq!(basic_header.chunk_type, 0);
        assert_eq!(basic_header.chunk_stream_id, 3);
    }

    #[test]
    fn test_basic_header_large() {
        let mock = MockTcpStream {
            cursor: io::Cursor::new(vec![0x0, 0x0]),
            buffer: Vec::new(),
        };
        let mut stream = MockRtmpMessageStream::new(mock);
        let basic_header = stream.read_chunk_basic_header().unwrap();
        assert_eq!(basic_header.chunk_type, 0);
        assert_eq!(basic_header.chunk_stream_id, 64);
    }

    fn send_message_header(
        stream: &mut MockRtmpMessageStream,
        header: ChunkMessageHeader,
        chunk_type: u8,
    ) {
        stream
            .send_chunk_basic_header(ChunkBasicHeader {
                chunk_stream_id: 3,
                chunk_type,
            })
            .unwrap();
        stream
            .send_chunk_message_header(header, chunk_type)
            .unwrap();
    }

    #[test]
    fn test_timestamp_type0_type3() {
        let mock = MockTcpStream {
            cursor: io::Cursor::new(vec![]),
            buffer: Vec::new(),
        };
        let mut stream = MockRtmpMessageStream::new(mock);
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 7122,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            0,
        );
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        stream.stream.consume_buffer();
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7122);
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7122 * 2);
    }

    #[test]
    fn test_timestamp_type0_type2_type3_type3() {
        let mock = MockTcpStream {
            cursor: io::Cursor::new(vec![]),
            buffer: Vec::new(),
        };
        let mut stream = MockRtmpMessageStream::new(mock);
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 7122,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            0,
        );
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 1,
            },
            2,
        );
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        stream.stream.consume_buffer();
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7122);
        assert_eq!(msg.header.timestamp_delta, 0);
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7123);
        assert_eq!(msg.header.timestamp_delta, 1);
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7124);
        assert_eq!(msg.header.timestamp_delta, 1);
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7125);
        assert_eq!(msg.header.timestamp_delta, 1);
    }

    #[test]
    fn test_timestamp_type0_type3_type3() {
        let mock = MockTcpStream {
            cursor: io::Cursor::new(vec![]),
            buffer: Vec::new(),
        };
        let mut stream = MockRtmpMessageStream::new(mock);
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 7122,
                message_length: 129,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            0,
        );
        stream.stream.write_all(&[0x0; 128]).unwrap();
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        stream.stream.write_all(&[0x0; 1]).unwrap();
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        stream.stream.write_all(&[0x0; 128]).unwrap();
        send_message_header(
            &mut stream,
            ChunkMessageHeader {
                timestamp: 0,
                message_length: 0,
                message_type_id: 0,
                message_stream_id: 0,
                timestamp_delta: 0,
            },
            3,
        );
        stream.stream.write_all(&[0x0; 129]).unwrap();
        stream.stream.consume_buffer();
        assert!(stream.read_message().unwrap().is_none());
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7122);
        assert_eq!(msg.header.message_length, 129);
        assert_eq!(msg.message.len(), 129);
        assert!(stream.read_message().unwrap().is_none());
        let msg = stream.read_message().unwrap().unwrap();
        assert_eq!(msg.header.timestamp, 7122 * 2);
        assert_eq!(msg.header.message_length, 129);
        assert_eq!(msg.message.len(), 129);
    }
}
