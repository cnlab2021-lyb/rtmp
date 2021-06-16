use super::error::{Error, Result};
use super::utils::*;
use std::collections::HashMap;
use std::io::Cursor;

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
// const LONG_STRING_MARKER: u8 = 0xC;
// const UNSUPPORTED_MARKER: u8 = 0xD;
// const RECORDSET_MARKER: u8 = 0xE;
// const XML_DOCUMENT_MARKER: u8 = 0xF;
// const TYPED_OBJECT_MARKER: u8 = 0x10;

#[derive(Debug, Clone, PartialEq)]
pub enum AmfObject {
    Number(f64),
    Boolean(bool),
    String(String),
    Object(HashMap<String, AmfObject>),
    Null,
    Undefined,
    Reference(u16),
    EcmaArray(Vec<(String, AmfObject)>),
    StrictArray(Vec<AmfObject>),
    Date((f64, i16)),
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

pub fn decode_amf_reference<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<u16> {
    if verify_marker {
        verify_type_marker(reader, REFERENCE_MARKER)?;
    }
    read_u16(reader).map_err(Error::Io)
}

pub fn decode_amf_strict_array<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<Vec<AmfObject>> {
    if verify_marker {
        verify_type_marker(reader, STRICT_ARRAY_MARKER)?;
    }
    (0..read_u32(reader).map_err(Error::Io)?)
        .map(|_| decode_amf_message(reader))
        .collect::<Result<Vec<_>>>()
}

pub fn decode_amf_date<T: AsRef<[u8]>>(
    reader: &mut Cursor<T>,
    verify_marker: bool,
) -> Result<(f64, i16)> {
    if verify_marker {
        verify_type_marker(reader, DATE_MARKER)?;
    }
    Ok((
        read_f64(reader).map_err(Error::Io)?,
        read_i16(reader).map_err(Error::Io)?,
    ))
}

pub fn decode_amf_message<T: AsRef<[u8]>>(reader: &mut Cursor<T>) -> Result<AmfObject> {
    let type_marker = read_u8(reader).map_err(Error::Io)?;
    match type_marker {
        NUMBER_MARKER => Ok(AmfObject::Number(decode_amf_number(reader, false)?)),
        BOOLEAN_MARKER => Ok(AmfObject::Boolean(decode_amf_boolean(reader, false)?)),
        STRING_MARKER => Ok(AmfObject::String(decode_amf_string(reader, false)?)),
        MOVIECLIP_MARKER => unreachable!("Movie clip marker is reserved"),
        OBJECT_MARKER => Ok(AmfObject::Object(decode_amf_object(reader, false)?)),
        NULL_MARKER => Ok(AmfObject::Null),
        UNDEFINED_MARKER => Ok(AmfObject::Undefined),
        REFERENCE_MARKER => Ok(AmfObject::Reference(decode_amf_reference(reader, false)?)),
        ECMA_ARRAY_MARKER => Ok(AmfObject::EcmaArray(decode_amf_ecma_array(reader, false)?)),
        OBJECT_END_MARKER => unreachable!("Object end marker should not appear on its own"),
        STRICT_ARRAY_MARKER => Ok(AmfObject::StrictArray(decode_amf_strict_array(
            reader, false,
        )?)),
        DATE_MARKER => Ok(AmfObject::Date(decode_amf_date(reader, false)?)),
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
        AmfObject::Number(ref x) => {
            message.push(NUMBER_MARKER);
            message.extend_from_slice(&x.to_be_bytes());
        }
        AmfObject::Boolean(ref b) => {
            message.push(BOOLEAN_MARKER);
            let byte = if *b { 1 } else { 0 };
            message.push(byte);
        }
        AmfObject::String(ref s) => {
            message.push(STRING_MARKER);
            message.extend_from_slice(&(s.len() as u16).to_be_bytes());
            message.extend_from_slice(s.as_bytes());
        }
        AmfObject::Object(ref obj) => {
            message.push(OBJECT_MARKER);
            obj.iter().for_each(|(key, val)| {
                message.extend_from_slice(&(key.len() as u16).to_be_bytes());
                message.extend_from_slice(key.as_bytes());
                encode_amf_message_impl(val, message);
            });
            message.extend_from_slice(&[0x0, 0x0, OBJECT_END_MARKER]);
        }
        AmfObject::Null => {
            message.push(NULL_MARKER);
        }
        AmfObject::Undefined => {
            message.push(UNDEFINED_MARKER);
        }
        AmfObject::Reference(ref r) => {
            message.push(REFERENCE_MARKER);
            message.extend_from_slice(&r.to_be_bytes());
        }
        AmfObject::EcmaArray(ref v) => {
            message.push(ECMA_ARRAY_MARKER);
            message.extend_from_slice(&(v.len() as u32).to_be_bytes());
            v.iter().for_each(|(key, val)| {
                message.extend_from_slice(&(key.len() as u16).to_be_bytes());
                message.extend_from_slice(key.as_bytes());
                encode_amf_message_impl(val, message);
            });
            message.extend_from_slice(&[0x0, 0x0, OBJECT_END_MARKER]);
        }
        AmfObject::StrictArray(ref v) => {
            message.push(STRICT_ARRAY_MARKER);
            message.extend_from_slice(&(v.len() as u32).to_be_bytes());
            v.iter().for_each(|t| encode_amf_message_impl(t, message));
        }
        AmfObject::Date((ref d, ref t)) => {
            message.push(DATE_MARKER);
            message.extend_from_slice(&d.to_be_bytes());
            message.extend_from_slice(&t.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::float_cmp)]
    #[test]
    fn amf_parse_number() {
        let mut reader = Cursor::new([NUMBER_MARKER; 9]);
        if let AmfObject::Number(x) = decode_amf_message(&mut reader).unwrap() {
            assert_eq!(x, 0_f64);
            assert_eq!(reader.position(), 9);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_bool_false() {
        let mut reader = Cursor::new([BOOLEAN_MARKER, 0x0]);
        if let AmfObject::Boolean(x) = decode_amf_message(&mut reader).unwrap() {
            assert_eq!(x, false);
            assert_eq!(reader.position(), 2);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_bool_true() {
        let mut reader = Cursor::new([BOOLEAN_MARKER, 0xA]);
        if let AmfObject::Boolean(x) = decode_amf_message(&mut reader).unwrap() {
            assert_eq!(x, true);
            assert_eq!(reader.position(), 2);
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_parse_string() {
        let mut reader = Cursor::new([STRING_MARKER, 0x00, 0x4, 0x6A, 0x69, 0x7A, 0x7A]);
        if let AmfObject::String(x) = decode_amf_message(&mut reader).unwrap() {
            assert_eq!(x, "jizz");
            assert_eq!(reader.position(), 7);
        } else {
            panic!("Test failed");
        }
    }

    #[allow(clippy::float_cmp)]
    #[test]
    fn amf_encode_number() {
        let buffer = encode_amf_messages(&[AmfObject::Number(7122.123_f64)]);
        if let AmfObject::Number(x) = decode_amf_message(&mut Cursor::new(buffer)).unwrap() {
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
        if let AmfObject::Object(amf) = decode_amf_message(&mut Cursor::new(buffer)).unwrap() {
            assert_eq!(amf.len(), object.len());
            for i in 1..5 {
                let key = format!("field{}", i);
                assert_eq!(object.get(&key), amf.get(&key));
            }
        } else {
            panic!("Test failed");
        }
    }

    #[test]
    fn amf_encode_ecma_array() {
        let array = vec![
            (
                String::from("key1"),
                AmfObject::String(String::from("val1")),
            ),
            (String::from("key2"), AmfObject::Boolean(true)),
            (String::from("key3"), AmfObject::Number(71.22_f64)),
            (String::from("key4"), AmfObject::Null),
        ];
        let buffer = encode_amf_messages(&[AmfObject::EcmaArray(array.clone())]);
        if let AmfObject::EcmaArray(v) = decode_amf_message(&mut Cursor::new(buffer)).unwrap() {
            eprintln!("array = {:?}, v = {:?}", array, v);
            assert_eq!(array, v);
        } else {
            panic!("Test failed");
        }
    }
}
