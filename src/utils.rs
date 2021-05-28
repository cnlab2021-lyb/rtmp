use std::io::{self, Read};

pub fn read_u8<R: Read>(reader: &mut R) -> io::Result<u8> {
    let mut buffer = [0x0; 1];
    reader.read_exact(&mut buffer)?;
    Ok(u8::from_be_bytes(buffer))
}

pub fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    let mut buffer = [0x0; 2];
    reader.read_exact(&mut buffer)?;
    Ok(u16::from_be_bytes(buffer))
}

pub fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut buffer = [0x0; 4];
    reader.read_exact(&mut buffer)?;
    Ok(u32::from_be_bytes(buffer))
}

pub fn read_f64<R: Read>(reader: &mut R) -> io::Result<f64> {
    let mut buffer = [0x0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(f64::from_be_bytes(buffer))
}

pub fn read_numeric<
    T: From<u8> + std::ops::Shl<u8, Output = T> + std::ops::BitOr<Output = T>,
    R: Read,
>(
    reader: &mut R,
    nbytes: usize,
) -> io::Result<T> {
    let mut buffer = vec![0; nbytes];
    reader.read_exact(&mut buffer)?;
    Ok(aggregate::<T>(&buffer, false))
}

pub fn aggregate<T: From<u8> + std::ops::Shl<u8, Output = T> + std::ops::BitOr<Output = T>>(
    buffer: &[u8],
    is_little_endian: bool,
) -> T {
    if buffer.is_empty() {
        T::from(0)
    } else if is_little_endian {
        combine::<T, _>(buffer.iter().rev())
    } else {
        combine::<T, _>(buffer.iter())
    }
}

pub fn combine<
    'a,
    T: From<u8> + std::ops::Shl<u8, Output = T> + std::ops::BitOr<Output = T>,
    I: Iterator<Item = &'a u8>,
>(
    mut iter: I,
) -> T {
    let first = *iter.next().unwrap_or(&0);
    iter.fold(T::from(first), |sum, &byte| (sum << 8) | T::from(byte))
}
