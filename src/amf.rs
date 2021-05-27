use std::collections::HashMap;
use std::io::{Error, ErrorKind, Read, Result};

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

#[derive(Debug, Clone)]
pub enum AmfObject {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(HashMap<String, AmfObject>),
    Null,
    Undefined,
    Reference(u16),
}

#[derive(Debug)]
pub struct AmfByteReader<'a> {
    buffer: &'a [u8],
    ptr: usize,
}

impl<'a> From<&'a [u8]> for AmfByteReader<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        AmfByteReader {
            buffer: bytes,
            ptr: 0,
        }
    }
}

impl<'a> From<&'a Vec<u8>> for AmfByteReader<'a> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        AmfByteReader {
            buffer: bytes,
            ptr: 0,
        }
    }
}

impl<'a> Read for AmfByteReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = buf.len();
        buf.copy_from_slice(&self.buffer[self.ptr..self.ptr + len]);
        self.ptr += len;
        Ok(len)
    }
}

impl<'a> AmfByteReader<'a> {
    fn read_one(&mut self) -> Result<u8> {
        if self.ptr == self.buffer.len() {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Invalid read when the buffer is empty"));
        }
        let result = self.buffer[self.ptr];
        self.ptr += 1;
        Ok(result)
    }

    pub fn finish(&self) -> bool {
        self.ptr == self.buffer.len()
    }
}

pub fn decode_amf_number(reader: &mut AmfByteReader, verify_marker: bool) -> Result<f64> {
    if verify_marker {
        let type_marker = reader.read_one()?;
        assert_eq!(type_marker, NUMBER_MARKER);
    }
    let mut buffer = [0x0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(f64::from_be_bytes(buffer))
}

pub fn decode_amf_object(
    reader: &mut AmfByteReader,
    verify_marker: bool,
) -> Result<HashMap<String, AmfObject>> {
    let mut map: HashMap<String, AmfObject> = HashMap::new();
    if verify_marker {
        let type_marker = reader.read_one()?;
        assert_eq!(type_marker, OBJECT_MARKER);
    }
    loop {
        let mut buffer = [0x0; 2];
        reader.read_exact(&mut buffer)?;
        let str_size = u16::from_be_bytes(buffer);
        if str_size == 0 {
            let next_type_marker = reader.read_one()?;
            assert_eq!(next_type_marker, OBJECT_END_MARKER);
            break;
        } else {
            let mut str_buffer = vec![0x0; str_size as usize];
            reader.read_exact(&mut str_buffer)?;
            map.insert(
                String::from_utf8(str_buffer).expect("UTF-8 string"),
                decode_amf_message(reader)?,
            );
        }
    }
    Ok(map)
}

pub fn decode_amf_null(reader: &mut AmfByteReader, verify_marker: bool) -> Result<()> {
    if verify_marker {
        let type_marker = reader.read_one()?;
        assert_eq!(type_marker, NULL_MARKER);
    }
    Ok(())
}

pub fn decode_amf_string(reader: &mut AmfByteReader, verify_marker: bool) -> Result<String> {
    if verify_marker {
        let type_marker = reader.read_one()?;
        assert_eq!(type_marker, STRING_MARKER);
    }
    let mut buffer = [0x0; 2];
    reader.read_exact(&mut buffer)?;
    let size = u16::from_be_bytes(buffer);
    let mut buffer = vec![0x0; size as usize];
    reader.read_exact(&mut buffer)?;
    Ok(String::from_utf8(buffer).expect("UTF-8 string"))
}

pub fn decode_amf_boolean(reader: &mut AmfByteReader, verify_marker: bool) -> Result<bool> {
    if verify_marker {
        let type_marker = reader.read_one()?;
        assert_eq!(type_marker, BOOLEAN_MARKER);
    }
    let mut buffer = [0x0; 1];
    reader.read_exact(&mut buffer)?;
    Ok(buffer[0] != 0)
}

pub fn decode_amf_message(reader: &mut AmfByteReader) -> Result<AmfObject> {
    let type_marker = reader.read_one()?;
    match type_marker {
        NUMBER_MARKER => Ok(AmfObject::Number(decode_amf_number(reader, false)?)),
        BOOLEAN_MARKER => Ok(AmfObject::Boolean(decode_amf_boolean(reader, false)?)),
        STRING_MARKER => Ok(AmfObject::String(decode_amf_string(reader, false)?)),
        OBJECT_MARKER => Ok(AmfObject::Object(decode_amf_object(reader, false)?)),
        NULL_MARKER => Ok(AmfObject::Null),
        _ => Err(Error::new(ErrorKind::InvalidData, "Invalid type marker")),
    }
}

pub fn decode_amf_message_from_slice(buffer: &[u8]) -> Result<AmfObject> {
    let mut reader = AmfByteReader::from(buffer);
    decode_amf_message(&mut reader)
}

fn encode_amf_message_impl(src: &AmfObject, message: &mut Vec<u8>) -> Result<()> {
    match src {
        AmfObject::String(s) => {
            message.push(STRING_MARKER);
            message.extend_from_slice(&(s.len() as u16).to_be_bytes());
            message.extend_from_slice(s.as_bytes());
        }
        AmfObject::Number(x) => {
            message.push(NUMBER_MARKER);
            message.extend_from_slice(&x.to_be_bytes());
        }
        AmfObject::Boolean(b) => {
            message.push(BOOLEAN_MARKER);
            let byte = match b {
                true => 1,
                false => 0,
            };
            message.push(byte);
        }
        AmfObject::Object(obj) => {
            message.push(OBJECT_MARKER);
            for (key, val) in obj.iter() {
                message.extend_from_slice(&(key.len() as u16).to_be_bytes());
                message.extend_from_slice(key.as_bytes());
                encode_amf_message_impl(val, message)?;
            }
            message.push(0x0);
            message.push(0x0);
            message.push(OBJECT_END_MARKER);
        }
        AmfObject::Null => {
            message.push(NULL_MARKER);
        }
        AmfObject::Undefined => {
            message.push(UNDEFINED_MARKER);
        }
        _ => {}
    }
    Ok(())
}

pub fn encode_amf_messages(src: &[AmfObject]) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    for obj in src {
        encode_amf_message_impl(obj, &mut buffer)?;
    }
    Ok(buffer)
}

#[test]
fn amf_parse_number() {
    let message = [0x0; 9];
    if let Ok(AmfObject::Number(x)) = decode_amf_message_from_slice(&message) {
        assert_eq!(x, 0_f64);
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_bool_false() {
    let message = [0x1, 0x0];
    if let Ok(AmfObject::Boolean(x)) = decode_amf_message_from_slice(&message) {
        assert_eq!(x, false);
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_bool_true() {
    let message = [0x1, 0xA];
    if let Ok(AmfObject::Boolean(x)) = decode_amf_message_from_slice(&message) {
        assert_eq!(x, true);
    } else {
        panic!();
    }
}

#[test]
fn amf_parse_string() {
    let message = [0x2, 0x00, 0x4, 0x6A, 0x69, 0x7A, 0x7A];
    if let Ok(AmfObject::String(x)) = decode_amf_message_from_slice(&message) {
        assert_eq!(x, "jizz");
    } else {
        panic!();
    }
}

#[test]
fn amf_encode_number() {
    match encode_amf_messages(&[AmfObject::Number(7122.123_f64)]) {
        Ok(buffer) => {
            if let Ok(AmfObject::Number(x)) = decode_amf_message_from_slice(&buffer) {
                assert_eq!(x, 7122.123_f64);
            } else {
                panic!();
            }
        }
        Err(_) => {
            panic!();
        }
    }
}

#[test]
fn amf_encode_object() {
    let object: HashMap<String, AmfObject> = [
        (
            "field1".to_string(),
            AmfObject::String("value1".to_string()),
        ),
        ("field2".to_string(), AmfObject::Number(255.0_f64)),
        ("field3".to_string(), AmfObject::Boolean(true)),
        ("field4".to_string(), AmfObject::Null),
    ]
    .iter()
    .cloned()
    .collect();
    match encode_amf_messages(&[AmfObject::Object(object.clone())]) {
        Ok(buffer) => {
            if let Ok(AmfObject::Object(x)) = decode_amf_message_from_slice(&buffer) {
                assert_eq!(x.len(), object.len());
                assert!(x.keys().all(|k| object.contains_key(k)));
            } else {
                panic!();
            }
        }
        Err(_) => {
            panic!();
        }
    }
}
