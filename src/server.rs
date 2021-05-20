use std::collections::HashMap;
use std::io::Result;
use std::net::TcpStream;

use super::amf::*;
use super::stream::*;

pub struct RtmpServer {
    stream: RtmpStream,
}

const RTMP_SET_CHUNK_SIZE: u8 = 0x1;
const RTMP_ABORT_MESSAGE: u8 = 0x2;
const RTMP_ACK: u8 = 0x3;
const RTMP_WINDOW_ACK_SIZE: u8 = 0x5;
const RTMP_SET_PEER_BANDWIDTH: u8 = 0x6;

const RTMP_COMMAND_MESSAGE_AMF0: u8 = 20;
const RTMP_COMMAND_MESSAGE_AMF3: u8 = 17;
const RTMP_DATA_MESSAGE_AMF0: u8 = 18;
const RTMP_DATA_MESSAGE_AMF3: u8 = 15;
const RTMP_VIDEO_MESSAGE: u8 = 8;
const RTMP_AUDIO_MESSAGE: u8 = 9;

const PROTOCOL_CONTROL_MESSAGE_STREAM_ID: u32 = 0;
const PROTOCOL_CONTROL_CHUNK_STREAM_ID: u16 = 0x2;

impl RtmpServer {
    #[allow(clippy::float_cmp)]
    fn handle_connect(
        &mut self,
        header: ChunkMessageHeader,
        mut reader: AmfByteReader,
    ) -> Result<()> {
        let transaction_id = decode_amf_message_number(&mut reader, true)?;
        assert_eq!(transaction_id, 1_f64);
        let cmd_object = decode_amf_message_object(&mut reader, true)?;
        eprintln!("cmd_object = {:?}", cmd_object);
        // TODO: Set window size, peer bandwidth, StreamBegin.

        // TODO: Fill properties and Information
        let properties: HashMap<String, AmfObject> = [
            (
                "fmsVer".to_string(),
                AmfObject::String("FMS/4,5,0,297".to_string()),
            ),
            ("capabilities".to_string(), AmfObject::Number(255.0_f64)),
        ]
        .iter()
        .cloned()
        .collect();
        let information: HashMap<String, AmfObject> =
            [("level".to_string(), AmfObject::String("status".to_string()))]
                .iter()
                .cloned()
                .collect();
        let buffer = encode_amf_messages(&[
            AmfObject::String("_result".to_string()),
            AmfObject::Number(1_f64),
            AmfObject::Object(properties),
            AmfObject::Object(information),
        ])?;
        eprintln!("send_message = {:?}", buffer);
        self.stream
            .send_message(3, header.message_stream_id, header.message_type_id, &buffer)?;
        Ok(())
    }

    // XXX: Seems to be undocumented.
    fn handle_release_stream(&mut self) -> Result<()> {
        Ok(())
    }

    // XXX: Seems to be undocumented.
    fn handle_fc_publish(&mut self) -> Result<()> {
        Ok(())
    }

    fn handle_create_stream(&mut self, mut reader: AmfByteReader) -> Result<()> {
        let transaction_id = decode_amf_message_number(&mut reader, true)?;
        let cmd_object = decode_amf_message_object(&mut reader, true)?;
        Ok(())
    }

    fn handle_command_message(&mut self, message: Message) -> Result<()> {
        let mut reader = AmfByteReader::from(&message.message);
        match decode_amf_message(&mut reader)? {
            AmfObject::String(cmd) => {
                eprintln!("cmd = {}", cmd);
                if cmd == "connect" {
                    self.handle_connect(message.header, reader)?;
                } else if cmd == "releaseStream" {
                    self.handle_release_stream()?;
                } else if cmd == "FCPublish" {
                    self.handle_fc_publish()?;
                } else if cmd == "createStream" {
                    self.handle_create_stream(reader)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_message(&mut self, message: Message) -> Result<()> {
        match message.header.message_type_id {
            RTMP_COMMAND_MESSAGE_AMF0 => {
                // AMF-0 encoded control message.
                self.handle_command_message(message)?;
            }
            RTMP_COMMAND_MESSAGE_AMF3 | RTMP_DATA_MESSAGE_AMF3 => {
                eprintln!("AMF-3 is not supported.");
            }
            _ => {}
        }
        Ok(())
    }

    pub fn serve(&mut self) -> Result<()> {
        self.stream.handle_handshake()?;
        loop {
            match self.stream.read_message()? {
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

    pub fn new(stream: TcpStream) -> RtmpServer {
        RtmpServer {
            stream: RtmpStream::new(stream),
        }
    }
}
