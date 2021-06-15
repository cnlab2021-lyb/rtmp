use super::error::{Error, Result};
use super::utils::*;
use std::collections::HashMap;
use std::io::Cursor;

const NUMBER_MARKER: u8 = 0x0;
const BOOLEAN_MARKER: u8 = 0x1;
const STRING_MARKER: u8 = 0x2;
const OBJECT_MARKER: u8 = 0x3;
// const MOVIECLIP_MARKER: u8 = 0x4;
const NULL_MARKER: u8 = 0x5;
// const UNDEFINED_MARKER: u8 = 0x6;
// const REFERENCE_MARKER: u8 = 0x7;
const ECMA_ARRAY_MARKER: u8 = 0x8;
const OBJECT_END_MARKER: u8 = 0x9;
// const STRICT_ARRAY_MARKER: u8 = 0xA;
// const DATE_MARKER: u8 = 0xB;
// const LONG_STRING_MARKER: u8 = 0xC;
// const UNSUPPORTED_MARKER: u8 = 0xD;
// const RECORDSET_MARKER: u8 = 0xE;
// const XML_DOCUMENT_MARKER: u8 = 0xF;
// const TYPED_OBJECT_MARKER: u8 = 0x10;

#[derive(Debug, Clone, PartialEq)]
pub enum AmfObject {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(HashMap<String, AmfObject>),
    EcmaArray(Vec<(String, AmfObject)>),
    Null,
    // Undefined,
    // Reference(u16),
}

fn verify_type_marker<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    expected_type_marker: u8,
) -> Result<()> {
    if read_u8(reader).map_err(Error::Io)? == expected_type_marker {
        Ok(())
    } else {
        Err(Error::AmfIncorrectTypeMarker)
    }
}

fn decode_amf_object_property<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
) -> Result<Option<(String, AmfObject)>> {
    let str_size = read_u16(reader).map_err(Error::Io)?;
    if str_size == 0 {
        return Ok(None);
    }
    Ok(Some((
        String::from_utf8(read_buffer(reader, str_size as usize).map_err(Error::Io)?)
            .expect("Invalid UTF-8 string"),
        decode_amf_message(reader)?,
    )))
}

pub fn decode_amf_number<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<f64> {
    if verify_marker {
        verify_type_marker(reader, NUMBER_MARKER)?;
    }
    read_f64(reader).map_err(Error::Io)
}

pub fn decode_amf_object<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<HashMap<String, AmfObject>> {
    if verify_marker {
        verify_type_marker(reader, OBJECT_MARKER)?;
    }
    let mut map: HashMap<String, AmfObject> = HashMap::new();
    loop {
        match decode_amf_object_property(reader)? {
            Some((key, value)) => {
                map.insert(key, value);
            }
            None => {
                verify_type_marker(reader, OBJECT_END_MARKER)?;
                break;
            }
        }
    }
    Ok(map)
}

pub fn decode_amf_null<T: AsRef<[u8]>>(reader: &mut Cursor<T>, verify_marker: bool) -> Result<()> {
    if verify_marker {
        verify_type_marker(reader, NULL_MARKER)?;
    }
    Ok(())
}

pub fn decode_amf_string<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<String> {
    if verify_marker {
        verify_type_marker(reader, STRING_MARKER)?;
    }
    let size = read_u16(reader).map_err(Error::Io)?;
    Ok(
        String::from_utf8(read_buffer(reader, size as usize).map_err(Error::Io)?)
            .expect("Invalid UTF-8 string"),
    )
}

pub fn decode_amf_boolean<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<bool> {
    if verify_marker {
        verify_type_marker(reader, BOOLEAN_MARKER)?;
    }
    Ok(read_u8(reader).map_err(Error::Io)? != 0)
}

pub fn decode_amf_ecma_array<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<Vec<(String, AmfObject)>> {
    if verify_marker {
        verify_type_marker(reader, ECMA_ARRAY_MARKER)?;
    }
    let mut result = Vec::new();
    for _ in 0..read_u32(reader).map_err(Error::Io)? {
        if let Some((key, value)) = decode_amf_object_property(reader)? {
            result.push((key, value));
        }
    }
    if decode_amf_object_property(reader)?.is_some() {
        return Err(Error::AmfIncorrectEndOfEcmaArray);
    }
    verify_type_marker(reader, OBJECT_END_MARKER)?;
    Ok(result)
}

pub fn decode_amf_message<T: AsRef<[u8]>>(reader: &mut Cursor<T>) -> Result<AmfObject> {
    let type_marker = read_u8(reader).map_err(Error::Io)?;
    match type_marker {
        NUMBER_MARKER => Ok(AmfObject::Number(decode_amf_number(reader, false)?)),
        BOOLEAN_MARKER => Ok(AmfObject::Boolean(decode_amf_boolean(reader, false)?)),
        STRING_MARKER => Ok(AmfObject::String(decode_amf_string(reader, false)?)),
        OBJECT_MARKER => Ok(AmfObject::Object(decode_amf_object(reader, false)?)),
        NULL_MARKER => Ok(AmfObject::Null),
        ECMA_ARRAY_MARKER => Ok(AmfObject::EcmaArray(decode_amf_ecma_array(reader, false)?)),
        _ => Err(Error::AmfIncorrectTypeMarker),
    }
}

pub fn encode_amf_messages(src: &[AmfObject]) -> Vec<u8> {
    let mut buffer = Vec::new();
    src.iter()
        .for_each(|obj| encode_amf_message_impl(obj, &mut buffer));
    buffer
}

fn encode_amf_message_impl(src: &AmfObject, message: &mut Vec<u8>) {
    match *src {
        AmfObject::String(ref s) => {
            message.push(STRING_MARKER);
            message.extend_from_slice(&(s.len() as u16).to_be_bytes());
            message.extend_from_slice(s.as_bytes());
        }
        AmfObject::Number(ref x) => {
            message.push(NUMBER_MARKER);
            message.extend_from_slice(&x.to_be_bytes());
        }
        AmfObject::Boolean(ref b) => {
            message.push(BOOLEAN_MARKER);
            let byte = if *b { 1 } else { 0 };
            message.push(byte);
        }
        AmfObject::Object(ref obj) => {
            message.push(OBJECT_MARKER);
            for (key, val) in obj.iter() {
                message.extend_from_slice(&(key.len() as u16).to_be_bytes());
                message.extend_from_slice(key.as_bytes());
                encode_amf_message_impl(val, message);
            }
            message.extend_from_slice(&[0x0, 0x0, OBJECT_END_MARKER]);
        }
        AmfObject::Null => {
            message.push(NULL_MARKER);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::float_cmp)]
    #[test]
    fn amf_parse_number() {
        let mut reader = Cursor::new([0x0; 9]);
        if let Ok(AmfObject::Number(x)) = decode_amf_message(&mut reader) {
            assert_eq!(x, 0_f64);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_bool_false() {
        let mut reader = Cursor::new([0x1, 0x0]);
        if let Ok(AmfObject::Boolean(x)) = decode_amf_message(&mut reader) {
            assert_eq!(x, false);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_bool_true() {
        let mut reader = Cursor::new([0x1, 0xA]);
        if let Ok(AmfObject::Boolean(x)) = decode_amf_message(&mut reader) {
            assert_eq!(x, true);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_string() {
        let mut reader = Cursor::new([0x2, 0x00, 0x4, 0x6A, 0x69, 0x7A, 0x7A]);
        if let Ok(AmfObject::String(x)) = decode_amf_message(&mut reader) {
            assert_eq!(x, "jizzz");
        } else {
            panic!("Test failed");
        }
    }

    #[allow(clippy::float_cmp)]
    #[test]
    fn amf_encode_number() {
        let buffer = encode_amf_messages(&[AmfObject::Number(7122.123_f64)]);
        if let Ok(AmfObject::Number(x)) = decode_amf_message(&mut Cursor::new(buffer)) {
            assert_eq!(x, 7122.123_f64);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_encode_object() {
        let object: HashMap<String, AmfObject> = [
            (
                String::from("field1"),
                AmfObject::String(String::from("value1")),
            ),
            (String::from("field2"), AmfObject::Number(255.0_f64)),
            (String::from("field3"), AmfObject::Boolean(true)),
            (String::from("field4"), AmfObject::Null),
        ]
        .iter()
        .cloned()
        .collect();
        let buffer = encode_amf_messages(&[AmfObject::Object(object.clone())]);
        if let Ok(AmfObject::Object(amf)) = decode_amf_message(&mut Cursor::new(buffer)) {
            assert_eq!(amf.len(), object.len());
            for i in 1..5 {
                let key = format!("field{}", i);
                assert_eq!(object.get(&key), amf.get(&key));
            }
        } else {
            panic!("Test failed");
        }
    }
}
