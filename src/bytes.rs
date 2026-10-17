use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

const RAW_BASE: u32 = 0x10FF00;
const RAW_FIRST: u32 = RAW_BASE + 0x80;
const RAW_PREFIX: [u8; 2] = [0xF4, 0x8F];

pub(crate) fn raw_char(byte: u8) -> char {
    if byte < 0x80 {
        char::from(byte)
    } else {
        char::from_u32(RAW_BASE + u32::from(byte)).unwrap_or('\u{fffd}')
    }
}

pub(crate) fn raw_byte(character: char) -> Option<u8> {
    let code = u32::from(character);
    (RAW_FIRST..=RAW_BASE + 0xFF)
        .contains(&code)
        .then(|| (code - RAW_BASE) as u8)
}

pub(crate) fn push_byte(text: &mut String, byte: u8) {
    text.push(raw_char(byte));
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        push_valid(&mut text, chunk.valid());
        for &byte in chunk.invalid() {
            push_byte(&mut text, byte);
        }
    }
    text
}

fn push_valid(text: &mut String, valid: &str) {
    if !has_raw_prefix(valid.as_bytes()) {
        text.push_str(valid);
        return;
    }
    for character in valid.chars() {
        if raw_byte(character).is_some() {
            let mut buffer = [0u8; 4];
            for &byte in character.encode_utf8(&mut buffer).as_bytes() {
                push_byte(text, byte);
            }
        } else {
            text.push(character);
        }
    }
}

fn has_raw_prefix(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|pair| pair == RAW_PREFIX)
}

pub(crate) fn encode(text: &str) -> Cow<'_, [u8]> {
    if !has_raw_prefix(text.as_bytes()) {
        return Cow::Borrowed(text.as_bytes());
    }
    let mut bytes = Vec::with_capacity(text.len());
    for character in text.chars() {
        match raw_byte(character) {
            Some(byte) => bytes.push(byte),
            None => {
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    Cow::Owned(bytes)
}

pub(crate) fn canonical(text: &str) -> String {
    match encode(text) {
        Cow::Borrowed(_) => text.to_string(),
        Cow::Owned(bytes) => decode(&bytes),
    }
}

pub(crate) fn lossy(text: &str) -> String {
    String::from_utf8_lossy(&encode(text)).into_owned()
}

pub(crate) fn from_os(text: &OsStr) -> String {
    decode(text.as_bytes())
}

pub(crate) fn to_os(text: &str) -> OsString {
    OsString::from_vec(encode(text).into_owned())
}

pub(crate) fn to_path(text: &str) -> PathBuf {
    PathBuf::from(to_os(text))
}

pub(crate) fn from_path(path: &Path) -> String {
    from_os(path.as_os_str())
}

pub(crate) fn read_file(path: &str) -> io::Result<String> {
    std::fs::read(to_path(path)).map(|bytes| decode(&bytes))
}

pub(crate) struct LineReader {
    reader: io::BufReader<std::fs::File>,
}

impl LineReader {
    pub(crate) fn open(path: &str) -> io::Result<LineReader> {
        let file = std::fs::File::open(to_path(path))?;
        if file.metadata()?.is_dir() {
            return Err(io::Error::from_raw_os_error(libc::EISDIR));
        }
        Ok(LineReader {
            reader: io::BufReader::new(out_of_the_way(file)),
        })
    }

    pub(crate) fn next_line(&mut self) -> Option<String> {
        let mut line = Vec::new();
        match io::BufRead::read_until(&mut self.reader, b'\n', &mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                if line.ends_with(b"\n") {
                    line.pop();
                }
                Some(decode(&line))
            }
        }
    }
}

fn out_of_the_way(file: std::fs::File) -> std::fs::File {
    use std::os::fd::{AsRawFd, FromRawFd};
    let high = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 255) };
    if high < 0 {
        return file;
    }
    unsafe { std::fs::File::from_raw_fd(high) }
}

pub(crate) struct Encoder<W: Write> {
    inner: W,
    pending: Vec<u8>,
}

impl<W: Write> Encoder<W> {
    pub(crate) fn new(inner: W) -> Self {
        Encoder {
            inner,
            pending: Vec::new(),
        }
    }

    pub(crate) fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    fn translate(&mut self, input: &[u8]) -> Vec<u8> {
        let mut joined = std::mem::take(&mut self.pending);
        joined.extend_from_slice(input);
        let split = incomplete_tail(&joined);
        self.pending = joined.split_off(split);
        if !has_raw_prefix(&joined) {
            return joined;
        }
        encode_raw_sequences(&joined)
    }
}

fn encode_raw_sequences(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match raw_sequence(&bytes[index..]) {
            Some(byte) => {
                output.push(byte);
                index += 4;
            }
            None => {
                output.push(bytes[index]);
                index += 1;
            }
        }
    }
    output
}

fn raw_sequence(bytes: &[u8]) -> Option<u8> {
    match bytes {
        [0xF4, 0x8F, third @ (0xBE | 0xBF), fourth @ 0x80..=0xBF, ..] => {
            Some(((third & 0x01) << 6 | (fourth & 0x3F)) + 0x80)
        }
        _ => None,
    }
}

fn incomplete_tail(bytes: &[u8]) -> usize {
    let start = bytes.len().saturating_sub(3);
    for index in start..bytes.len() {
        let tail = &bytes[index..];
        let width = match tail[0] {
            0xF0..=0xF7 => 4,
            0xE0..=0xEF => 3,
            0xC0..=0xDF => 2,
            _ => continue,
        };
        if tail.len() < width && tail[1..].iter().all(|byte| byte & 0xC0 == 0x80) {
            return index;
        }
    }
    bytes.len()
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let translated = self.translate(buffer);
        self.inner.write_all(&translated)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            self.inner.write_all(&pending)?;
        }
        self.inner.flush()
    }
}

impl<W: Write> Drop for Encoder<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_is_unchanged() {
        assert_eq!(decode("héllo wörld".as_bytes()), "héllo wörld");
        assert_eq!(&*encode("héllo"), "héllo".as_bytes());
    }

    #[test]
    fn invalid_bytes_round_trip() {
        let input = b"a\xffb\xe9c\xc3";
        let text = decode(input);
        assert_eq!(text.chars().count(), 6);
        assert_eq!(&*encode(&text), input);
    }

    #[test]
    fn every_byte_round_trips() {
        let input: Vec<u8> = (0..=255).collect();
        assert_eq!(&*encode(&decode(&input)), &input[..]);
    }

    #[test]
    fn real_private_use_characters_round_trip() {
        let input = "\u{10FF80}\u{10FFFF}x".as_bytes();
        let text = decode(input);
        assert_eq!(text.chars().count(), 9);
        assert_eq!(&*encode(&text), input);
    }

    #[test]
    fn pushed_bytes_become_utf8_when_canonical() {
        let mut text = String::new();
        push_byte(&mut text, 0xC3);
        push_byte(&mut text, 0xA9);
        assert_eq!(&*encode(&text), "é".as_bytes());
        assert_eq!(canonical(&text), "é");
        let mut lone = String::new();
        push_byte(&mut lone, 0xFF);
        assert_eq!(canonical(&lone), lone);
        assert_eq!(lossy(&lone), "\u{fffd}");
    }

    #[test]
    fn os_strings_round_trip() {
        let os = OsStr::from_bytes(b"file\xff.txt");
        assert_eq!(to_os(&from_os(os)), os);
    }

    #[test]
    fn encoder_translates_split_writes() {
        let text = decode(b"x\xff\xfey");
        let encoded = text.as_bytes();
        let mut output = Vec::new();
        {
            let mut encoder = Encoder::new(&mut output);
            for byte in encoded {
                encoder.write_all(std::slice::from_ref(byte)).unwrap();
            }
        }
        assert_eq!(output, b"x\xff\xfey");
    }

    #[test]
    fn line_reader_decodes_each_line() {
        let path = std::env::temp_dir().join(format!("grfsh_line_reader_{}", std::process::id()));
        std::fs::write(&path, b"one\xe9\r\ntwo\nthree").unwrap();
        let mut reader = LineReader::open(&from_path(&path)).unwrap();
        assert_eq!(&*encode(&reader.next_line().unwrap()), b"one\xe9\r");
        assert_eq!(reader.next_line().unwrap(), "two");
        assert_eq!(reader.next_line().unwrap(), "three");
        assert!(reader.next_line().is_none());
        let _ = std::fs::remove_file(&path);
        assert!(LineReader::open("/").is_err());
    }

    #[test]
    fn encoder_passes_through_invalid_input() {
        let mut output = Vec::new();
        {
            let mut encoder = Encoder::new(&mut output);
            encoder.write_all(b"a\xf4").unwrap();
        }
        assert_eq!(output, b"a\xf4");
    }
}
