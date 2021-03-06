use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    // IO errors
    Io(std::io::Error),

    // RTMP chunk stream errors
    HandshakeCorrupted,
    InvalidTimestamp,
    UnknownMessageTypeId(u8),

    // RTMP message stream errors
    NonStringCommand,
    UnexpectedAmfObjectType,
    UnknownDataMessage,
    UnknownCommandMessage(String),
    InconsistentMessageLength,
    MissingMediaStream,

    // AMF errors
    Amf3NotSupported,
    AmfIncorrectTypeMarker,
    AmfIncorrectEndOfEcmaArray,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::Io(ref e) => fmt::Display::fmt(e, f),
            Error::HandshakeCorrupted => {
                write!(f, "RTMP handshake failed with incorrect random digest")
            }
            Error::InvalidTimestamp => write!(f, ""),
            Error::UnknownMessageTypeId(ref id) => write!(f, "Unknown message type ID: {}", id),
            Error::NonStringCommand => write!(
                f,
                "Receive AMF command message starting with non-string object"
            ),
            Error::UnexpectedAmfObjectType => write!(f, "Receive unexpected AMF object type"),
            Error::UnknownCommandMessage(ref msg) => {
                write!(f, "Unknown AMF-0 command message: {}", msg)
            }

            Error::Amf3NotSupported => write!(f, "AMF-3 encoded messages are not supported"),
            Error::AmfIncorrectTypeMarker => write!(f, "Receive unexpected AMF type marker"),
            Error::AmfIncorrectEndOfEcmaArray => {
                write!(f, "Expect end-of-object marker at the end of ECMA array")
            }
            _ => Ok(()),
        }
    }
}
