use std::fmt::{Debug, Display};
use std::ops::Range;

pub struct ConstStr<const N: usize> {
    len: usize,
    buf: [u8; N]
}

impl<const N: usize> ConstStr<N> {
    /// Create a new [`ConstStr`] using the given backing buffer, containing a string of the specified length.
    /// 
    /// Note that the string can be shorter than its backing buffer.
    pub const fn new(buf: [u8; N], len: usize) -> Self {
        let slice = const_slice(&buf, 0..len);

        if std::str::from_utf8(slice).is_err() {
            panic!("Tried to create a non-UTF8 ConstStr")
        }
        
        Self {  len, buf }
    }
}

impl<const N: usize> AsRef<str> for ConstStr<N> {
    fn as_ref(&self) -> &str {
        // Safety:
        // ConstStr creation does a UTF-8 validity check.
        unsafe {
            std::str::from_utf8_unchecked(&self.buf[0..self.len])
        }
    }
}

impl<const N: usize> Debug for ConstStr<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}

impl<const N:  usize> Display for ConstStr<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

/// Copy a slice of `T` into an array of `[T; N]`, starting from the specified index.
/// 
/// Due to certain limitations on const evaluation, the destination array cannot be mutated by reference -
/// it has to be moved into the function, then moved back out.
/// 
/// The last index is also returned, to allow composing const_copy calls.
pub const fn const_copy<T: Copy, const N: usize>(src: &[T], mut dst: [T; N], start: usize) -> ([T; N], usize) {
    let mut i = 0;
    
    while i < src.len() {
        dst[start + i] = src[i];
        
        i += 1
    }
    
    (dst, i + start) 
}

/// Slice into an `[&T]` in a constant context.
pub const fn const_slice<T>(slice: &[T], range: Range<usize>) -> &[T] {
    let mut slice = slice;
    let mut range = range;

    while range.start != 0 {
        slice = match slice {
            [_first, rest @ ..] => rest,
            _ => panic!("Index out of bounds"),
        };
        
        range.start -= 1;
        range.end -= 1;
    }
    
    loop {
        if slice.len() == range.end {
            return slice;
        }
        
        slice = match slice {
            [rest @ .., _last] => rest,
            _ => panic!("Index out of bounds"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn invalid_unicode() {
        // This will slice halfway through a 4-byte codepoint, creating invalid Unicode.
        // ConstStr's new method should catch this.
        let _ = ConstStr::new(*b"\xF0\x90\x80\x80", 2);
    }

    #[test]
    fn const_create() {
        const TEST: ConstStr<13> = ConstStr::new(*b"Hello, World!", 13);

        assert_eq!("Hello, World!", TEST.as_ref())
    }
}