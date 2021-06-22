// RTMP protocol control messages
pub const RTMP_SET_CHUNK_SIZE: u8 = 0x1;
pub const RTMP_ABORT_MESSAGE: u8 = 0x2;
pub const RTMP_ACKNOWLEDGEMENT: u8 = 0x3;
pub const RTMP_USER_CONTROL_MESSAGE: u8 = 0x4;
pub const RTMP_WINDOW_ACK_SIZE: u8 = 0x5;
pub const RTMP_SET_PEER_BANDWIDTH: u8 = 0x6;

// RTMP command messages
pub const RTMP_COMMAND_MESSAGE_AMF0: u8 = 20;
pub const RTMP_COMMAND_MESSAGE_AMF3: u8 = 17;
pub const RTMP_DATA_MESSAGE_AMF0: u8 = 18;
pub const RTMP_DATA_MESSAGE_AMF3: u8 = 15;
pub const RTMP_AUDIO_MESSAGE: u8 = 8;
pub const RTMP_VIDEO_MESSAGE: u8 = 9;

pub const RTMP_NET_CONNECTION_STREAM_ID: u32 = 0;

pub const RTMP_PROTOCOL_CONTROL_MESSAGE_STREAM_ID: u32 = 0;
pub const RTMP_PROTOCOL_CONTROL_CHUNK_STREAM_ID: u16 = 0x2;

// RTMP user control message events
pub const RTMP_USER_CONTROL_SET_BUFFER_LENGTH: u16 = 0x3;
