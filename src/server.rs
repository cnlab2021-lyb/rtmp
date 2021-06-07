use std::collections::HashMap;
use std::io::Cursor;
use std::net::TcpStream;

use super::amf::*;
use super::error::{Error, Result};
use super::stream::*;

pub struct RtmpServer {
    stream: RtmpStream,
}

const RTMP_SET_CHUNK_SIZE: u8 = 0x1;
// const RTMP_ABORT_MESSAGE: u8 = 0x2;
const RTMP_ACKNOWLEDGEMENT: u8 = 0x3;
const RTMP_USER_CONTROL_MESSAGE: u8 = 0x4;
const RTMP_WINDOW_ACK_SIZE: u8 = 0x5;
const RTMP_SET_PEER_BANDWIDTH: u8 = 0x6;

const RTMP_COMMAND_MESSAGE_AMF0: u8 = 20;
const RTMP_COMMAND_MESSAGE_AMF3: u8 = 17;
const RTMP_DATA_MESSAGE_AMF0: u8 = 18;
const RTMP_DATA_MESSAGE_AMF3: u8 = 15;
const RTMP_AUDIO_MESSAGE: u8 = 8;
const RTMP_VIDEO_MESSAGE: u8 = 9;

const RTMP_NET_CONNECTION_STREAM_ID: u32 = 0;

const RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID: u32 = 0;
const RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID: u16 = 0x2;

impl RtmpServer {
    #[allow(clippy::float_cmp)]
    fn handle_connect(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        assert_eq!(transaction_id, 1_f64);
        let cmd_object = decode_amf_object(&mut reader, true)?;
        eprintln!("cmd_object = {:?}", cmd_object);
        // TODO: Set window size, peer bandwidth.

        {
            self.stream.send_message(
                RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
                RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
                RTMP_ACKNOWLEDGEMENT,
                &7122_u32.to_be_bytes(),
            )?;
            let mut buffer = Vec::from(1048576_u32.to_be_bytes());
            // Send acknowledgement.
            // Set window acknowledgement size.
            self.stream.send_message(
                RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
                RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
                RTMP_WINDOW_ACK_SIZE,
                &buffer,
            )?;
            buffer.push(2);
            // Set peer bandwidth.
            self.stream.send_message(
                RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
                RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
                RTMP_SET_PEER_BANDWIDTH,
                &buffer,
            )?;
            // Send user control message: StreamBegin.
            self.stream.send_message(
                RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
                RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
                RTMP_USER_CONTROL_MESSAGE,
                &[0x0; 6],
            )?;
        }

        // TODO: Fill properties and Information.
        let properties: HashMap<String, AmfObject> = [
            (
                "fmsVer".to_string(),
                AmfObject::String("FMS/4,5,0,297".to_string()),
            ),
            ("capabilities".to_string(), AmfObject::Number(255.0_f64)),
            ("mode".to_string(), AmfObject::Number(1.0)),
        ]
        .iter()
        .cloned()
        .collect();
        let information: HashMap<String, AmfObject> = [
            ("level".to_string(), AmfObject::String("status".to_string())),
            (
                "code".to_string(),
                AmfObject::String("NetConnection.Connect.Success".to_string()),
            ),
            (
                "description".to_string(),
                AmfObject::String("Connection succeeded.".to_string()),
            ),
            ("objectEncoding".to_string(), AmfObject::Number(0.0)),
        ]
        .iter()
        .cloned()
        .collect();
        let buffer = encode_amf_messages(&[
            AmfObject::String("_result".to_string()),
            AmfObject::Number(1_f64),
            AmfObject::Object(properties),
            AmfObject::Object(information),
        ]);
        self.stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            RTMP_COMMAND_MESSAGE_AMF0,
            &buffer,
        )?;
        Ok(())
    }

    fn handle_release_stream(&self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let _ = decode_amf_number(&mut reader, true)?;
        let _ = decode_amf_null(&mut reader, true)?;
        let _ = decode_amf_string(&mut reader, true)?;
        Ok(())
    }

    fn handle_fc_publish(&self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let _ = decode_amf_number(&mut reader, true)?;
        let _ = decode_amf_null(&mut reader, true)?;
        let _ = decode_amf_string(&mut reader, true)?;
        Ok(())
    }

    fn handle_create_stream(
        &mut self,
        mut reader: Cursor<Vec<u8>>,
        header: ChunkMessageHeader,
    ) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        let cmd_object = decode_amf_message(&mut reader)?;
        match cmd_object {
            AmfObject::Object(_) | AmfObject::Null => {
                self.stream.send_message(
                    3,
                    RTMP_NET_CONNECTION_STREAM_ID,
                    RTMP_COMMAND_MESSAGE_AMF0,
                    &encode_amf_messages(&[
                        AmfObject::String("_result".to_string()),
                        AmfObject::Number(transaction_id),
                        AmfObject::Null,
                        AmfObject::Number(header.message_stream_id as f64),
                    ]),
                )?;
                Ok(())
            }
            _ => Err(Error::UnexpectedAmfObjectType),
        }
    }

    fn on_status(code: &str) -> Vec<u8> {
        let information: HashMap<String, AmfObject> = [
            ("level".to_string(), AmfObject::String("status".to_string())),
            ("code".to_string(), AmfObject::String(code.to_string())),
        ]
        .iter()
        .cloned()
        .collect();
        encode_amf_messages(&[
            AmfObject::String("onStatus".to_string()),
            AmfObject::Number(0_f64),
            AmfObject::Null,
            AmfObject::Object(information),
        ])
    }

    fn handle_play(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let _transaction_id = decode_amf_number(&mut reader, true)?;
        // assert_eq!(transaction_id, 0_f64);
        let _cmd_object = decode_amf_null(&mut reader, true)?;
        let stream_name = decode_amf_string(&mut reader, true)?;
        let start = decode_amf_message(&mut reader);
        let duration = decode_amf_message(&mut reader);
        let reset = decode_amf_message(&mut reader);
        eprintln!(
            "stream_name = {}, start = {:?}, duration = {:?}, reset = {:?}",
            stream_name, start, duration, reset
        );
        todo!()
    }

    fn handle_publish(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let _transaction_id = decode_amf_number(&mut reader, true)?;
        // assert_eq!(transaction_id, 0_f64);
        let _cmd_object = decode_amf_null(&mut reader, true)?;
        let publishing_name = decode_amf_string(&mut reader, true)?;
        let publishing_type = decode_amf_string(&mut reader, true)?;
        eprintln!(
            "publishing_name = {}, publishing_type = {}",
            publishing_name, publishing_type
        );
        self.stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            RTMP_COMMAND_MESSAGE_AMF0,
            &Self::on_status("NetStream.Publish.Start"),
        )?;
        Ok(())
    }

    fn handle_delete_stream(&mut self, _reader: Cursor<Vec<u8>>) -> Result<()> {
        Ok(())
    }

    fn handle_command_message(&mut self, message: Message) -> Result<bool> {
        let mut reader = Cursor::new(message.message);
        if let AmfObject::String(cmd) = decode_amf_message(&mut reader)? {
            eprintln!("cmd = {}", cmd);
            match cmd.as_str() {
                "connect" => self.handle_connect(reader)?,
                "deleteStream" => self.handle_delete_stream(reader)?,
                "releaseStream" => self.handle_release_stream(reader)?,
                "FCPublish" => self.handle_fc_publish(reader)?,
                "createStream" => self.handle_create_stream(reader, message.header)?,
                "play" => self.handle_play(reader)?,
                "publish" => self.handle_publish(reader)?,
                _ => {}
            }
            Ok(cmd == "deleteStream")
        } else {
            Err(Error::NonStringCommand)
        }
    }

    fn handle_data_message(&mut self, message: Message) -> Result<()> {
        eprintln!("Handle data message");
        let mut reader = Cursor::new(message.message);
        if decode_amf_string(&mut reader, true)? != "@setDataFrame" {
            return Err(Error::UnknownDataMessage);
        }
        if decode_amf_string(&mut reader, true)? != "onMetaData" {
            return Err(Error::UnknownDataMessage);
        }
        let prop = decode_amf_ecma_array(&mut reader, true)?;
        eprintln!("{:?}", prop);
        Ok(())
    }

    fn handle_set_chunk_size(&mut self, message: Message) {
        assert_eq!(message.header.message_length, 4);
        let mut buffer = [0x0; 4];
        buffer.copy_from_slice(&message.message);
        self.stream.max_chunk_size_read = u32::from_be_bytes(buffer) as usize;
        // The most-significant bit should not be set.
        assert_eq!(self.stream.max_chunk_size_read >> 31, 0);
    }

    fn handle_message(&mut self, message: Message) -> Result<bool> {
        match message.header.message_type_id {
            RTMP_COMMAND_MESSAGE_AMF0 => {
                // AMF-0 encoded control message.
                if self.handle_command_message(message)? {
                    return Ok(true);
                }
            }
            RTMP_DATA_MESSAGE_AMF0 => {
                // AMF-0 encoded data message.
                self.handle_data_message(message)?;
            }
            RTMP_COMMAND_MESSAGE_AMF3 | RTMP_DATA_MESSAGE_AMF3 => {
                return Err(Error::Amf3NotSupported);
            }
            RTMP_SET_CHUNK_SIZE => {
                self.handle_set_chunk_size(message);
            }
            RTMP_AUDIO_MESSAGE => {}
            RTMP_VIDEO_MESSAGE => {}
            _ => {}
        }
        Ok(false)
    }

    pub fn serve(&mut self) -> Result<()> {
        self.stream.handle_handshake()?;
        loop {
            if let Some(message) = self.stream.read_message()? {
                eprintln!("message.header = {:?}", message.header);
                if message.message.len() != message.header.message_length {
                    return Err(Error::InconsistentMessageLength);
                }
                if self.handle_message(message)? {
                    return Ok(());
                }
            }
        }
    }

    pub fn new(stream: TcpStream) -> RtmpServer {
        RtmpServer {
            stream: RtmpStream::new(stream),
        }
    }
}
