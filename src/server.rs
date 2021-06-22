use std::collections::HashMap;
use std::io::Cursor;
use std::net::TcpStream;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::amf::*;
use crate::error::{Error, Result};
use crate::stream::{ChunkMessageHeader, Message, RtmpMessageStream};
use crate::utils::*;
use crate::constant::*;

#[derive(Debug)]
pub struct RtmpClient {
    stream: Arc<Mutex<RtmpMessageStream<TcpStream>>>,
    paused: bool,
}

impl RtmpClient {
    fn new(stream: Arc<Mutex<RtmpMessageStream<TcpStream>>>) -> Self {
        Self {
            stream,
            paused: false,
        }
    }
}

#[derive(Default, Debug)]
pub struct RtmpMediaStream {
    clients: Vec<RtmpClient>,
    metadata: Option<Message>,
    published: bool,
}

impl Deref for RtmpMediaStream {
    type Target = Vec<RtmpClient>;

    fn deref(&self) -> &Self::Target {
        &self.clients
    }
}

impl DerefMut for RtmpMediaStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.clients
    }
}

pub struct RtmpServer {
    message_stream: Arc<Mutex<RtmpMessageStream<TcpStream>>>,
    media_streams: Arc<Mutex<HashMap<String, RtmpMediaStream>>>,
    stream_name: String,
}

impl RtmpMediaStream {
    fn broadcast(&mut self, timestamp: u32, type_id: u8, message: &Message) {
        let offline: Vec<_> = self
            .clients
            .iter()
            .enumerate()
            .map(|(i, client)| {
                if client.paused {
                    return None;
                }
                let stream = &mut *client.stream.lock().unwrap();
                if stream
                    .send_message(
                        3,
                        message.header.message_stream_id,
                        timestamp,
                        type_id,
                        &message.message,
                    )
                    .is_err()
                {
                    Some(i)
                } else {
                    None
                }
            })
            .flatten()
            .collect();

        // Remove offline clients
        offline.iter().for_each(|i| {
            self.clients.remove(*i);
        });
    }
}

impl RtmpServer {
    #[allow(clippy::float_cmp)]
    fn handle_connect(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        assert_eq!(transaction_id, 1_f64);
        let cmd_object = decode_amf_object(&mut reader, true)?;
        eprintln!("cmd_object = {:?}", cmd_object);
        let stream = &mut *self.message_stream.lock().unwrap();
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_ACKNOWLEDGEMENT,
            &7122_u32.to_be_bytes(),
        )?;
        let mut buffer = Vec::from(1048576_u32.to_be_bytes());
        // Set window acknowledgement size.
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_WINDOW_ACK_SIZE,
            &buffer,
        )?;
        buffer.push(2);
        // Set peer bandwidth.
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_SET_PEER_BANDWIDTH,
            &buffer,
        )?;
        // Send user control message: Stream Begin.
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_USER_CONTROL_MESSAGE,
            &[0x0; 6],
        )?;

        // TODO: Fill properties and Information.
        let properties: HashMap<String, AmfObject> = [
            (
                String::from("fmsVer"),
                AmfObject::String(String::from("FMS/4,5,0,297")),
            ),
            (String::from("capabilities"), AmfObject::Number(255.0_f64)),
            (String::from("mode"), AmfObject::Number(1.0)),
        ]
        .iter()
        .cloned()
        .collect();
        let information: HashMap<String, AmfObject> = [
            (
                String::from("level"),
                AmfObject::String(String::from("status")),
            ),
            (
                String::from("code"),
                AmfObject::String(String::from("NetConnection.Connect.Success")),
            ),
            (String::from("objectEncoding"), AmfObject::Number(0.0)),
        ]
        .iter()
        .cloned()
        .collect();
        let buffer = encode_amf_messages(&[
            AmfObject::String(String::from("_result")),
            AmfObject::Number(1_f64),
            AmfObject::Object(properties),
            AmfObject::Object(information),
        ]);
        stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
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

    fn handle_create_stream(
        &mut self,
        mut reader: Cursor<Vec<u8>>,
        header: ChunkMessageHeader,
    ) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        let cmd_object = decode_amf_message(&mut reader)?;
        match cmd_object {
            AmfObject::Object(_) | AmfObject::Null => {
                self.message_stream.lock().unwrap().send_message(
                    3,
                    RTMP_NET_CONNECTION_STREAM_ID,
                    0,
                    RTMP_COMMAND_MESSAGE_AMF0,
                    &encode_amf_messages(&[
                        AmfObject::String(String::from("_result")),
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
            (
                String::from("level"),
                AmfObject::String(String::from("status")),
            ),
            (String::from("code"), AmfObject::String(code.to_string())),
        ]
        .iter()
        .cloned()
        .collect();
        encode_amf_messages(&[
            AmfObject::String(String::from("onStatus")),
            AmfObject::Number(0_f64),
            AmfObject::Null,
            AmfObject::Object(information),
        ])
    }

    fn handle_play(
        &mut self,
        _header: ChunkMessageHeader,
        mut reader: Cursor<Vec<u8>>,
    ) -> Result<()> {
        let _transaction_id = decode_amf_number(&mut reader, true)?;
        // assert_eq!(_transaction_id, 0_f64);
        let _cmd_object = decode_amf_null(&mut reader, true)?;
        let stream_name = decode_amf_string(&mut reader, true)?;
        let start = decode_amf_message(&mut reader);
        let duration = decode_amf_message(&mut reader);
        let reset = decode_amf_message(&mut reader);
        eprintln!(
            "stream_name = {}, start = {:?}, duration = {:?}, reset = {:?}",
            stream_name, start, duration, reset
        );
        let stream = &mut *self.message_stream.lock().unwrap();
        // Set chunk size.
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_SET_CHUNK_SIZE,
            &(0x7FFFFFFF_u32).to_be_bytes(),
        )?;
        stream.max_chunk_size_write = 0x7FFFFFFF;

        // Send user control message: Stream Begin.
        stream.send_message(
            RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID,
            RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID,
            0,
            RTMP_USER_CONTROL_MESSAGE,
            &[0x0; 6],
        )?;

        stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
            RTMP_COMMAND_MESSAGE_AMF0,
            &Self::on_status("NetStream.Play.Reset"),
        )?;
        stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
            RTMP_COMMAND_MESSAGE_AMF0,
            &Self::on_status("NetStream.Play.Start"),
        )?;
        // XXX: Unknown message
        stream.send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
            RTMP_DATA_MESSAGE_AMF0,
            &encode_amf_messages(&[
                AmfObject::String(String::from("|RtmpSampleAccess")),
                AmfObject::Boolean(true),
                AmfObject::Boolean(true),
            ]),
        )?;
        stream.set_read_timeout(Duration::from_micros(1));
        let media_streams = &mut *self.media_streams.lock().unwrap();
        let media_streams = media_streams
            .entry(stream_name.clone())
            .or_insert_with(RtmpMediaStream::default);

        // Stream has already begun, send metadata first.
        if let Some(ref metadata) = media_streams.metadata {
            stream.send_message(
                3,
                metadata.header.message_stream_id,
                metadata.header.timestamp,
                RTMP_DATA_MESSAGE_AMF0,
                &metadata.message,
            )?;
        }
        media_streams.push(RtmpClient::new(Arc::clone(&self.message_stream)));
        self.stream_name = stream_name;
        Ok(())
    }

    #[allow(clippy::float_cmp)]
    fn handle_pause(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        assert_eq!(transaction_id, 0_f64);
        let _ = decode_amf_null(&mut reader, true)?;
        let pause = decode_amf_boolean(&mut reader, true)?;
        let _pause_time = decode_amf_number(&mut reader, true)?;
        let media_streams = &mut *self.media_streams.lock().unwrap();
        if let Some(media_stream) = media_streams.get_mut(&self.stream_name) {
            media_stream.clients.iter_mut().for_each(|client| {
                if Arc::ptr_eq(&client.stream, &self.message_stream) {
                    client.paused = pause;
                }
            });
        }
        self.message_stream.lock().unwrap().send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
            RTMP_COMMAND_MESSAGE_AMF0,
            &Self::on_status("NetStream.Pause.Notify"),
        )?;
        Ok(())
    }

    fn handle_publish(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let _transaction_id = decode_amf_number(&mut reader, true)?;
        // assert_eq!(_transaction_id, 0_f64);
        let _cmd_object = decode_amf_null(&mut reader, true)?;
        let publishing_name = decode_amf_string(&mut reader, true)?;
        let publishing_type = decode_amf_string(&mut reader, true)?;
        eprintln!(
            "publishing_name = {}, publishing_type = {}",
            publishing_name, publishing_type
        );
        let media_streams = &mut *self.media_streams.lock().unwrap();
        let entry = media_streams
            .entry(publishing_name.clone())
            .or_insert_with(RtmpMediaStream::default);
        let code = if entry.published {
            "NetStream.Publish.Denied"
        } else {
            entry.published = true;
            "NetStream.Publish.Start"
        };
        self.message_stream.lock().unwrap().send_message(
            3,
            RTMP_NET_CONNECTION_STREAM_ID,
            0,
            RTMP_COMMAND_MESSAGE_AMF0,
            &Self::on_status(code),
        )?;
        self.stream_name = publishing_name;
        Ok(())
    }

    fn handle_delete_stream(&mut self, _reader: Cursor<Vec<u8>>) -> Result<()> {
        self.media_streams.lock().unwrap().remove(&self.stream_name);
        Ok(())
    }

    #[allow(clippy::float_cmp)]
    fn handle_get_stream_length(&mut self, mut reader: Cursor<Vec<u8>>) -> Result<()> {
        let transaction_id = decode_amf_number(&mut reader, true)?;
        assert_eq!(transaction_id, 3_f64);
        let _ = decode_amf_null(&mut reader, true)?;
        let _stream_name = decode_amf_string(&mut reader, true)?;
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
                "createStream" => self.handle_create_stream(reader, message.header)?,
                "play" => self.handle_play(message.header, reader)?,
                "pause" => self.handle_pause(reader)?,
                "getStreamLength" => self.handle_get_stream_length(reader)?,
                "publish" => self.handle_publish(reader)?,
                "FCPublish" | "FCUnpublish" => {}
                _ => return Err(Error::UnknownCommandMessage(cmd)),
            }
            Ok(cmd == "deleteStream")
        } else {
            Err(Error::NonStringCommand)
        }
    }

    fn handle_data_message(&mut self, message: Message) -> Result<()> {
        let mut reader = Cursor::new(&message.message);
        if decode_amf_string(&mut reader, true)? != "@setDataFrame" {
            return Err(Error::UnknownDataMessage);
        }
        if decode_amf_string(&mut reader, true)? != "onMetaData" {
            return Err(Error::UnknownDataMessage);
        }
        let properties = decode_amf_ecma_array(&mut reader, true)?;
        eprintln!("{:?}", properties);

        self.broadcast(0, RTMP_DATA_MESSAGE_AMF0, &message)?;
        let media_streams = &mut *self.media_streams.lock().unwrap();
        let media_stream = media_streams
            .get_mut(&self.stream_name)
            .ok_or(Error::MissingMediaStream)?;
        media_stream.metadata = Some(message);
        Ok(())
    }

    fn handle_set_chunk_size(&mut self, message: Message) {
        assert_eq!(message.header.message_length, 4);
        let mut buffer = [0x0; 4];
        buffer.copy_from_slice(&message.message);
        self.message_stream.lock().unwrap().max_chunk_size_read =
            u32::from_be_bytes(buffer) as usize;
    }

    fn handle_window_ack_size(&mut self, message: Message) {
        assert_eq!(message.header.message_length, 4);
        let mut buffer = [0x0; 4];
        buffer.copy_from_slice(&message.message);
        let window_ack_size = u32::from_be_bytes(buffer);
        eprintln!("window ack size = {}", window_ack_size);
    }

    fn handle_user_control_message(&mut self, message: Message) -> Result<()> {
        let mut cursor = Cursor::new(message.message);
        let event_type = read_u16(&mut cursor).map_err(Error::Io)?;
        match event_type {
            RTMP_USER_CONTROL_SET_BUFFER_LENGTH => {
                let stream_id = read_u32(&mut cursor).map_err(Error::Io)?;
                let buffer_length = read_u32(&mut cursor).map_err(Error::Io)?;
                eprintln!(
                    "stream_id = {}, buffer_length = {}",
                    stream_id, buffer_length
                );
            }
            _ => {
                eprintln!("event type = {}", event_type);
            }
        }
        Ok(())
    }

    fn broadcast(&mut self, timestamp: u32, type_id: u8, message: &Message) -> Result<()> {
        let media_streams = &mut *self.media_streams.lock().unwrap();
        let s = media_streams
            .get_mut(&self.stream_name)
            .ok_or(Error::MissingMediaStream)?;
        s.broadcast(timestamp, type_id, message);
        Ok(())
    }

    fn handle_video_message(&mut self, message: Message) -> Result<()> {
        let (_frame_type, _codec_id) = ((message.message[0] >> 4) & 0xf, message.message[0] & 0xf);
        self.broadcast(message.header.timestamp, RTMP_VIDEO_MESSAGE, &message)?;
        Ok(())
    }

    fn handle_audio_message(&mut self, message: Message) -> Result<()> {
        self.broadcast(message.header.timestamp, RTMP_AUDIO_MESSAGE, &message)?;
        Ok(())
    }

    fn handle_abort_message(&mut self, message: Message) -> Result<()> {
        let chunk_stream_id = read_u32(&mut Cursor::new(message.message)).map_err(Error::Io)?;
        let stream = &mut *self.message_stream.lock().unwrap();
        stream.channels.remove(&(chunk_stream_id as u16));
        Ok(())
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
            RTMP_ABORT_MESSAGE => {
                self.handle_abort_message(message)?;
            }
            RTMP_WINDOW_ACK_SIZE => {
                self.handle_window_ack_size(message);
            }
            RTMP_USER_CONTROL_MESSAGE => {
                self.handle_user_control_message(message)?;
            }
            RTMP_AUDIO_MESSAGE => {
                self.handle_audio_message(message)?;
            }
            RTMP_VIDEO_MESSAGE => {
                self.handle_video_message(message)?;
            }
            RTMP_ACKNOWLEDGEMENT => {
                let ack = read_u32(&mut Cursor::new(message.message)).map_err(Error::Io)?;
                eprintln!("ack = {}", ack);
            }
            _ => {
                return Err(Error::UnknownMessageTypeId(message.header.message_type_id));
            }
        }
        Ok(false)
    }

    pub fn serve(&mut self) -> Result<()> {
        self.message_stream.lock().unwrap().handle_handshake()?;
        loop {
            let message = self.message_stream.lock().unwrap().read_message();
            match message {
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Ok(None) => {}
                Err(e) => {
                    return Err(e);
                }
                Ok(Some(msg)) => {
                    if msg.message.len() != msg.header.message_length {
                        return Err(Error::InconsistentMessageLength);
                    }
                    if self.handle_message(msg)? {
                        return Ok(());
                    }
                }
            }
            std::thread::yield_now();
        }
    }

    pub fn new(
        stream: TcpStream,
        media_streams: Arc<Mutex<HashMap<String, RtmpMediaStream>>>,
    ) -> RtmpServer {
        RtmpServer {
            message_stream: Arc::new(Mutex::new(RtmpMessageStream::new(stream))),
            media_streams,
            stream_name: String::new(),
        }
    }
}
