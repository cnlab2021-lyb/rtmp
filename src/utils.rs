use std::io::{self, Read};
use std::ops;
use std::os::unix::io::RawFd;

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

pub fn read_i16<R: Read>(reader: &mut R) -> io::Result<i16> {
    Ok(i16::from_be_bytes(read_buffer_sized::<_, 2>(reader)?))
}

pub fn read_numeric<T, R: Read>(reader: &mut R, nbytes: usize) -> io::Result<T>
where
    T: From<u8> + std::ops::Shl<u8, Output = T> + std::ops::BitOr<Output = T>,
{
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

pub unsafe fn get_fd_stat(fd: RawFd) -> (libc::dev_t, libc::ino_t) {
    eprintln!("fd = {}", fd);
    let mut stat: libc::stat = std::mem::zeroed();
    let stat_ptr: *mut libc::stat = &mut stat;
    libc::fstat(fd, stat_ptr);
    (stat.st_dev, stat.st_ino)
}

pub fn aggregate<T>(buffer: &[u8], is_little_endian: bool) -> T
where
    T: From<u8> + ops::Shl<u8, Output = T> + ops::BitOr<Output = T>,
{
    fn combine<'a, T, I>(mut iter: I) -> T
    where
        T: From<u8> + ops::Shl<u8, Output = T> + ops::BitOr<Output = T>,
        I: Iterator<Item = &'a u8>,
    {
        let first = *iter.next().unwrap_or(&0);
        iter.fold(T::from(first), |sum, &byte| (sum << 8) | T::from(byte))
    }

    if buffer.is_empty() {
        T::from(0)
    } else if is_little_endian {
        combine::<T, _>(buffer.iter().rev())
    } else {
        combine::<T, _>(buffer.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_u8() {
        let mut cursor = Cursor::new([0x0, 0x1, 0x2, 0x3]);
        for i in 0..4 {
            assert_eq!(read_u8(&mut cursor).unwrap(), i);
        }
    }

    #[test]
    fn test_read_u16() {
        let mut cursor = Cursor::new([0x0, 0x1, 0x2, 0x3]);
        assert_eq!(read_u16(&mut cursor).unwrap(), 0x0001_u16);
        assert_eq!(read_u16(&mut cursor).unwrap(), 0x0203_u16);
    }

    #[test]
    fn test_read_u32() {
        let mut cursor = Cursor::new([0x0, 0x1, 0x2, 0x3]);
        assert_eq!(read_u32(&mut cursor).unwrap(), 0x00010203_u32);
    }

    #[allow(clippy::float_cmp)]
    #[test]
    fn test_read_f64() {
        let mut cursor = Cursor::new(71.22_f64.to_be_bytes());
        assert_eq!(read_f64(&mut cursor).unwrap(), 71.22_f64);
    }
}
