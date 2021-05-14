use std::collections::HashMap;
use std::io::{Read, Result, Error, ErrorKind};

const NUMBER_MARKER: u8 = 0x0;
const BOOLEAN_MARKER: u8 = 0x1;
const STRING_MARKER: u8 = 0x2;
const OBJECT_MARKER: u8 = 0x3;
const MOVIECLIP_MARKER: u8 = 0x4;
const NULL_MARKER: u8 = 0x5;
const UNDEFINED_MARKER: u8 = 0x6;
const REFERENCE_MARKER: u8 = 0x7;
const ECMA_ARRAY_MARKER: u8 = 0x8;
const OBJECT_END_MARKER: u8 = 0x9;
const STRICT_ARRAY_MARKER: u8 = 0xA;
const DATE_MARKER: u8 = 0xB;
const LONG_STRING_MARKER: u8 = 0xC;
const UNSUPPORTED_MARKER: u8 = 0xD;
const RECORDSET_MARKER: u8 = 0xE;
const XML_DOCUMENT_MARKER: u8 = 0xF;
const TYPED_OBJECT_MARKER: u8 = 0x10;

pub enum AmfObject {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(HashMap<String, AmfObject>),
    Null,
    Undefined,
    Reference(u16),
}

struct ByteReader<'a> {
    buffer: &'a [u8],
    ptr: usize,
}

impl<'a> From<&'a [u8]> for ByteReader<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        ByteReader {
            buffer: bytes,
            ptr: 0,
        }
    }
}

impl<'a> Read for ByteReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = buf.len();
        buf.copy_from_slice(&self.buffer[self.ptr..self.ptr + len]);
        self.ptr += len;
        Ok(len)
    }
}

impl<'a> ByteReader<'a> {
    fn read_one(&mut self) -> Result<u8> {
        let mut buffer = [0x0; 1];
        self.read(&mut buffer)?;
        Ok(buffer[0])
    }
}

fn decode_amf_message_impl(reader: &mut ByteReader) -> Result<AmfObject> {
    let type_marker = reader.read_one()?;
    match type_marker {
        NUMBER_MARKER => {
            let mut buffer = [0x0; 8];
            reader.read(&mut buffer)?;
            Ok(AmfObject::Number(f64::from_be_bytes(buffer)))
        }
        BOOLEAN_MARKER => {
            let byte = reader.read_one()?;
            Ok(AmfObject::Boolean(byte != 0))
        }
        STRING_MARKER => {
            let mut buffer = [0x0; 2];
            reader.read(&mut buffer)?;
            let size = u16::from_be_bytes(buffer);
            let mut buffer = vec![0x0; size as usize];
            reader.read(&mut buffer)?;
            Ok(AmfObject::String(String::from_utf8(buffer).map_err(|_| Error::new(ErrorKind::InvalidData, "error"))?))
        }
        _ => Err(Error::new(ErrorKind::InvalidData, "error")),
    }
}

pub fn decode_amf_message(message: &[u8]) -> Result<AmfObject> {
    let mut reader = ByteReader::from(message);
    decode_amf_message_impl(&mut reader)
}

#[test]
fn amf_parse_number() {
    let message = [0x0; 9];
    if let Ok(AmfObject::Number(x)) = decode_amf_message(&message) {
        assert_eq!(x, 0_f64); 
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_bool_false() {
    let message = [0x1, 0x0];
    if let Ok(AmfObject::Boolean(x)) = decode_amf_message(&message) {
        assert_eq!(x, false); 
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_bool_true() {
    let message = [0x1, 0xA];
    if let Ok(AmfObject::Boolean(x)) = decode_amf_message(&message) {
        assert_eq!(x, true); 
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_string() {
    let message = [0x2, 0x00, 0x4, 0x6A, 0x69, 0x7A, 0x7A];
    if let Ok(AmfObject::String(x)) = decode_amf_message(&message) {
        assert_eq!(x, "jizz"); 
    } else {
        panic!();
    }
}
