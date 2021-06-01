use std::io::{self, Read};

pub fn read_u8<R: Read>(reader: &mut R) -> io::Result<u8> {
    Ok(u8::from_be_bytes(read_buffer_sized::<_, 1>(reader)?))
}

pub fn read_u16<R: Read>(reader: &mut R) -> io::Result<u16> {
    Ok(u16::from_be_bytes(read_buffer_sized::<_, 2>(reader)?))
}

pub fn read_u32<R: Read>(reader: &mut R) -> io::Result<u32> {
    Ok(u32::from_be_bytes(read_buffer_sized::<_, 4>(reader)?))
}

pub fn read_f64<R: Read>(reader: &mut R) -> io::Result<f64> {
    Ok(f64::from_be_bytes(read_buffer_sized::<_, 8>(reader)?))
}

pub fn read_numeric<
    T: From<u8> + std::ops::Shl<u8, Output = T> + std::ops::BitOr<Output = T>,
    R: Read,
>(
    reader: &mut R,
    nbytes: usize,
) -> io::Result<T> {
    Ok(aggregate::<T>(&read_buffer(reader, nbytes)?, false))
}

pub fn read_buffer<R: Read>(reader: &mut R, nbytes: usize) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0x0; nbytes];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
}

pub fn read_buffer_sized<R: Read, const N: usize>(reader: &mut R) -> io::Result<[u8; N]> {
    let mut buffer = [0x0; N];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
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
