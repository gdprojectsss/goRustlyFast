use std::io::Write;

use crate::expand::{DEFAULT_IFS, split_ifs};
use crate::shell::{self, Shell};
use crate::syntax::parser::is_name;
use crate::sys;
use crate::variables::{Array, Key};

const MAPFILE_USAGE: &str = "mapfile: usage: mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]";
const READ_USAGE: &str = "read: usage: read [-Eers] [-a array] [-d delim] [-i text] [-n nchars] [-N nchars] [-p prompt] [-t timeout] [-u fd] [name ...]";
const DEFAULT_QUANTUM: usize = 5000;

enum OptionError {
    Invalid(char),
    Missing(char),
}

type ParsedOptions = (Vec<(char, String)>, Vec<String>);

fn parse_options(args: &[String], spec: &str) -> Result<ParsedOptions, OptionError> {
    let mut options = Vec::new();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if argument.len() < 2 || !argument.starts_with('-') {
            break;
        }
        index += 1;
        let letters: Vec<char> = argument[1..].chars().collect();
        for (position, letter) in letters.iter().enumerate() {
            let Some(found) = spec.find(*letter) else {
                return Err(OptionError::Invalid(*letter));
            };
            if !spec[found + 1..].starts_with(':') {
                options.push((*letter, String::new()));
                continue;
            }
            let attached: String = letters[position + 1..].iter().collect();
            let value = if attached.is_empty() {
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or(OptionError::Missing(*letter))?;
                index += 1;
                value
            } else {
                attached
            };
            options.push((*letter, value));
            break;
        }
    }
    Ok((options, args[index..].to_vec()))
}

fn report_option_error(command: &str, usage: &str, error: OptionError) -> i32 {
    match error {
        OptionError::Invalid(letter) => {
            shell::error(format!("{command}: -{letter}: invalid option"))
        }
        OptionError::Missing(letter) => {
            shell::error(format!("{command}: -{letter}: option requires an argument"))
        }
    }
    eprintln!("{usage}");
    2
}

fn parse_fd(command: &str, value: &str) -> Result<i32, i32> {
    let Ok(fd) = value.parse::<i32>() else {
        shell::error(format!(
            "{command}: {value}: invalid file descriptor specification"
        ));
        return Err(1);
    };
    if fd < 0 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        let error = std::io::Error::from_raw_os_error(libc::EBADF);
        shell::error(format!(
            "{command}: {value}: invalid file descriptor: {}",
            sys::error_message(&error)
        ));
        return Err(1);
    }
    Ok(fd)
}

fn decode(bytes: &[u8]) -> String {
    crate::bytes::decode(bytes)
}

fn utf8_length(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

struct FdReader {
    fd: i32,
    buffer: Vec<u8>,
    position: usize,
    seekable: bool,
}

impl FdReader {
    fn new(fd: i32) -> Self {
        let seekable = unsafe { libc::lseek(fd, 0, libc::SEEK_CUR) } >= 0;
        FdReader {
            fd,
            buffer: Vec::new(),
            position: 0,
            seekable,
        }
    }

    fn byte(&mut self) -> Option<u8> {
        if self.position < self.buffer.len() {
            self.position += 1;
            return Some(self.buffer[self.position - 1]);
        }
        let size = if self.seekable { 65536 } else { 1 };
        self.buffer.resize(size, 0);
        self.position = 0;
        match sys::read(self.fd, &mut self.buffer) {
            Ok(count) if count > 0 => {
                self.buffer.truncate(count);
                self.position = 1;
                Some(self.buffer[0])
            }
            _ => {
                self.buffer.clear();
                None
            }
        }
    }
}

impl Drop for FdReader {
    fn drop(&mut self) {
        let unread = self.buffer.len() - self.position;
        if self.seekable && unread > 0 {
            unsafe {
                libc::lseek(self.fd, -(unread as libc::off_t), libc::SEEK_CUR);
            }
        }
    }
}

struct ReadOptions {
    raw: bool,
    silent: bool,
    prompt: Option<String>,
    array: Option<String>,
    count: Option<usize>,
    exact: bool,
    timeout: Option<f64>,
    delimiter: u8,
    fd: i32,
}

fn read_options(args: &[String]) -> Result<(ReadOptions, Vec<String>), i32> {
    let (parsed, names) = parse_options(args, "a:d:eEi:n:N:p:rst:u:")
        .map_err(|error| report_option_error("read", READ_USAGE, error))?;
    let mut options = ReadOptions {
        raw: false,
        silent: false,
        prompt: None,
        array: None,
        count: None,
        exact: false,
        timeout: None,
        delimiter: b'\n',
        fd: 0,
    };
    for (letter, value) in parsed {
        match letter {
            'r' => options.raw = true,
            's' => options.silent = true,
            'p' => options.prompt = Some(value),
            'a' => options.array = Some(value),
            'd' => options.delimiter = value.bytes().next().unwrap_or(0),
            'n' | 'N' => {
                let Ok(count) = value.parse::<usize>() else {
                    shell::error(format!("read: {value}: invalid number"));
                    return Err(1);
                };
                options.count = Some(count);
                options.exact = letter == 'N';
            }
            't' => match value.parse::<f64>() {
                Ok(seconds) if seconds >= 0.0 && seconds.is_finite() => {
                    options.timeout = Some(seconds.min(1e9));
                }
                _ => {
                    shell::error(format!("read: {value}: invalid timeout specification"));
                    return Err(1);
                }
            },
            'u' => options.fd = parse_fd("read", &value)?,
            _ => {}
        }
    }
    for name in names.iter().chain(options.array.iter()) {
        if !is_name(name) {
            shell::error(format!("read: `{name}': not a valid identifier"));
            return Err(1);
        }
    }
    Ok((options, names))
}

enum Ending {
    Delimiter,
    EndOfInput,
    TimedOut,
    Count,
}

fn read_character(reader: &mut FdReader, first: u8, bytes: &mut Vec<u8>) {
    bytes.push(first);
    for _ in 1..utf8_length(first) {
        match reader.byte() {
            Some(byte) => bytes.push(byte),
            None => break,
        }
    }
}

fn read_input(options: &ReadOptions) -> (Vec<u8>, Ending) {
    let deadline = options
        .timeout
        .map(|seconds| std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds));
    let mut reader = FdReader::new(options.fd);
    reader.seekable &= deadline.is_none();
    let mut bytes = Vec::new();
    let mut characters = 0;
    loop {
        if options.count.is_some_and(|count| characters >= count) {
            return (bytes, Ending::Count);
        }
        if let Some(deadline) = deadline
            && reader.position >= reader.buffer.len()
        {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if !sys::wait_readable(options.fd, remaining) {
                return (bytes, Ending::TimedOut);
            }
        }
        let Some(byte) = reader.byte() else {
            return (bytes, Ending::EndOfInput);
        };
        if !options.exact && byte == options.delimiter {
            return (bytes, Ending::Delimiter);
        }
        if !options.raw && byte == b'\\' {
            match reader.byte() {
                Some(b'\n') => continue,
                Some(next) => {
                    bytes.push(b'\\');
                    read_character(&mut reader, next, &mut bytes);
                }
                None => return (bytes, Ending::EndOfInput),
            }
        } else {
            read_character(&mut reader, byte, &mut bytes);
        }
        characters += 1;
    }
}

fn ifs(shell: &Shell) -> String {
    shell
        .get_variable("IFS")
        .unwrap_or_else(|| DEFAULT_IFS.into())
}

pub(super) fn read(shell: &mut Shell, args: &[String]) -> i32 {
    let (options, names) = match read_options(args) {
        Ok(parsed) => parsed,
        Err(status) => return status,
    };
    if options.timeout == Some(0.0) {
        let ready = sys::wait_readable(options.fd, std::time::Duration::ZERO);
        return i32::from(!ready);
    }
    if let Some(prompt) = &options.prompt
        && sys::isatty(options.fd)
    {
        let _ = std::io::stderr().write_all(&crate::bytes::encode(prompt));
        let _ = std::io::stderr().flush();
    }

    let echo_guard =
        (options.silent && sys::isatty(options.fd)).then(|| sys::disable_echo(options.fd));
    let (bytes, ending) = read_input(&options);
    if let Some(Some(saved)) = echo_guard {
        sys::restore_terminal(options.fd, &saved);
        eprintln!();
    }

    let mut line = decode(&bytes);
    if !options.raw {
        line = remove_backslashes(&line);
    }

    let result = if let Some(array) = &options.array {
        let fields = if options.exact {
            vec![line]
        } else {
            split_ifs(&line, &ifs(shell), None)
        };
        shell.set_array(array, Array::indexed(fields))
    } else if names.is_empty() {
        shell.try_set_variable("REPLY", &line)
    } else if options.exact {
        assign_whole(shell, &names, &line)
    } else {
        assign_fields(shell, &names, &line)
    };
    if let Err(message) = result {
        shell::error(format!("read: {message}"));
        return 1;
    }
    match ending {
        Ending::TimedOut => 142,
        Ending::EndOfInput => 1,
        Ending::Delimiter | Ending::Count => 0,
    }
}

fn remove_backslashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn assign_whole(shell: &mut Shell, names: &[String], line: &str) -> Result<(), String> {
    for (index, name) in names.iter().enumerate() {
        shell.try_set_variable(name, if index == 0 { line } else { "" })?;
    }
    Ok(())
}

fn assign_fields(shell: &mut Shell, names: &[String], line: &str) -> Result<(), String> {
    let fields = split_ifs(line, &ifs(shell), Some(names.len()));
    for (index, name) in names.iter().enumerate() {
        shell.try_set_variable(name, fields.get(index).map_or("", String::as_str))?;
    }
    Ok(())
}

struct MapfileOptions {
    delimiter: u8,
    limit: Option<usize>,
    origin: Option<i64>,
    skip: usize,
    trim: bool,
    fd: i32,
    callback: Option<String>,
    quantum: usize,
}

fn count_value(value: &str, problem: &str) -> Result<i64, i32> {
    value
        .parse::<i64>()
        .ok()
        .filter(|count| *count >= 0)
        .ok_or_else(|| {
            shell::error(format!("mapfile: {value}: {problem}"));
            1
        })
}

fn mapfile_options(args: &[String]) -> Result<(MapfileOptions, Vec<String>), i32> {
    let (parsed, rest) = parse_options(args, "d:n:O:s:tu:C:c:")
        .map_err(|error| report_option_error("mapfile", MAPFILE_USAGE, error))?;
    let mut options = MapfileOptions {
        delimiter: b'\n',
        limit: None,
        origin: None,
        skip: 0,
        trim: false,
        fd: 0,
        callback: None,
        quantum: DEFAULT_QUANTUM,
    };
    for (letter, value) in parsed {
        match letter {
            'd' => options.delimiter = value.bytes().next().unwrap_or(0),
            'n' => {
                let count = count_value(&value, "invalid line count")? as usize;
                options.limit = (count > 0).then_some(count);
            }
            'O' => options.origin = Some(count_value(&value, "invalid array origin")?),
            's' => options.skip = count_value(&value, "invalid line count")? as usize,
            't' => options.trim = true,
            'u' => options.fd = parse_fd("mapfile", &value)?,
            'C' => options.callback = Some(value),
            _ => match value.parse::<usize>() {
                Ok(quantum) if quantum > 0 => options.quantum = quantum,
                _ => {
                    shell::error(format!("mapfile: {value}: invalid callback quantum"));
                    return Err(1);
                }
            },
        }
    }
    Ok((options, rest))
}

fn read_record(reader: &mut FdReader, delimiter: u8) -> Option<Vec<u8>> {
    let mut record = Vec::new();
    loop {
        match reader.byte() {
            Some(byte) => {
                record.push(byte);
                if byte == delimiter {
                    return Some(record);
                }
            }
            None if record.is_empty() => return None,
            None => return Some(record),
        }
    }
}

fn single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn mapfile(shell: &mut Shell, args: &[String]) -> i32 {
    let (options, rest) = match mapfile_options(args) {
        Ok(parsed) => parsed,
        Err(status) => return status,
    };
    let name = rest.first().cloned().unwrap_or_else(|| "MAPFILE".into());
    if !is_name(&name) {
        shell::error(format!("mapfile: `{name}': not a valid identifier"));
        return 1;
    }
    let mut array = match (options.origin, shell.arrays.get(&name)) {
        (Some(_), Some(existing)) if !existing.is_associative() => existing.clone(),
        (_, Some(existing)) if existing.is_associative() => {
            shell::error(format!("mapfile: {name}: not an indexed array"));
            return 1;
        }
        _ => Array::indexed(Vec::new()),
    };
    let origin = options.origin.unwrap_or(0);
    let mut reader = FdReader::new(options.fd);
    let mut stored = 0usize;
    let mut seen = 0usize;
    while options.limit.is_none_or(|limit| stored < limit) {
        let Some(mut record) = read_record(&mut reader, options.delimiter) else {
            break;
        };
        seen += 1;
        if seen <= options.skip {
            continue;
        }
        if options.trim && record.last() == Some(&options.delimiter) {
            record.pop();
        }
        let line = decode(&record);
        let index = origin + stored as i64;
        if let Some(callback) = &options.callback
            && (stored + 1).is_multiple_of(options.quantum)
        {
            drop(reader);
            if let Err(message) = shell.set_array(&name, array.clone()) {
                shell::error(format!("mapfile: {message}"));
                return 1;
            }
            shell.run_source(&format!("{callback} {index} {}", single_quote(&line)));
            reader = FdReader::new(options.fd);
        }
        if let Err(message) = array.set(Key::Index(index), line) {
            shell::error(format!("mapfile: {message}"));
            return 1;
        }
        stored += 1;
    }
    match shell.set_array(&name, array) {
        Ok(()) => 0,
        Err(message) => {
            shell::error(format!("mapfile: {message}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_split_with_remainder_in_last_name() {
        let mut shell = Shell::new();
        let names = vec!["first".to_string(), "rest".to_string()];
        assign_fields(&mut shell, &names, "  one two  three ").unwrap();
        assert_eq!(shell.get_variable("first").as_deref(), Some("one"));
        assert_eq!(shell.get_variable("rest").as_deref(), Some("two  three"));
    }

    #[test]
    fn missing_fields_are_empty() {
        let mut shell = Shell::new();
        let names = vec!["first".to_string(), "second".to_string()];
        assign_fields(&mut shell, &names, "only").unwrap();
        assert_eq!(shell.get_variable("second").as_deref(), Some(""));
    }

    #[test]
    fn backslashes_escape_the_next_character() {
        assert_eq!(remove_backslashes("a\\ b\\\\c"), "a b\\c");
    }

    #[test]
    fn options_parse_like_getopt() {
        let args: Vec<String> = ["-tn5", "-d", ",", "name"]
            .iter()
            .map(|argument| argument.to_string())
            .collect();
        let Ok((options, rest)) = parse_options(&args, "d:n:t") else {
            panic!("options should parse");
        };
        assert_eq!(
            options,
            vec![
                ('t', String::new()),
                ('n', "5".to_string()),
                ('d', ",".to_string())
            ]
        );
        assert_eq!(rest, vec!["name".to_string()]);
        assert!(parse_options(&args, "t").is_err());
    }
}
