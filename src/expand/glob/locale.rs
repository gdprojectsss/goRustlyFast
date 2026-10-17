use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::ffi::CString;

thread_local! {
    static ACTIVE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    static BYTE_COLLATION: Cell<bool> = const { Cell::new(true) };
    static BYTE_CHARACTERS: Cell<bool> = const { Cell::new(false) };
}

fn first_set(shell: &crate::shell::Shell, names: [&'static str; 3]) -> (&'static str, String) {
    names
        .iter()
        .filter_map(|name| shell.get_variable(name).map(|value| (*name, value)))
        .find(|(_, value)| !value.is_empty())
        .unwrap_or(("", String::new()))
}

fn warn_if_invalid(category: libc::c_int, variable: &str, value: &str) {
    if !value.is_empty() && !is_byte_order(value) && !apply(category, value) {
        crate::shell::error(format!(
            "warning: setlocale: {variable}: cannot change locale ({value})"
        ));
    }
}

fn is_byte_order(name: &str) -> bool {
    name == "C" || name == "POSIX" || name.starts_with("C.") || name.starts_with("POSIX.")
}

pub fn configure(shell: &crate::shell::Shell) {
    let (collation_variable, collation) = first_set(shell, ["LC_ALL", "LC_COLLATE", "LANG"]);
    let (characters_variable, characters) = first_set(shell, ["LC_ALL", "LC_CTYPE", "LANG"]);
    let wanted = (collation, characters);
    if ACTIVE.with(|active| active.borrow().as_ref() == Some(&wanted)) {
        return;
    }
    let (collation, characters) = &wanted;
    warn_if_invalid(libc::LC_CTYPE, characters_variable, characters);
    if collation_variable != characters_variable {
        warn_if_invalid(libc::LC_COLLATE, collation_variable, collation);
    }
    BYTE_CHARACTERS.with(|flag| flag.set(matches!(characters.as_str(), "C" | "POSIX")));
    let byte_collation = collation.is_empty()
        || is_byte_order(collation)
        || !apply(libc::LC_COLLATE, collation)
        || !apply(libc::LC_CTYPE, collation_characters(collation, characters));
    if byte_collation {
        apply(libc::LC_COLLATE, "C");
        apply(libc::LC_CTYPE, "C");
    }
    BYTE_COLLATION.with(|flag| flag.set(byte_collation));
    ACTIVE.with(|active| *active.borrow_mut() = Some(wanted));
}

fn collation_characters<'a>(collation: &'a str, characters: &'a str) -> &'a str {
    if characters.is_empty() || is_byte_order(characters) {
        collation
    } else {
        characters
    }
}

pub fn byte_characters() -> bool {
    BYTE_CHARACTERS.with(Cell::get)
}

fn apply(category: libc::c_int, name: &str) -> bool {
    let Ok(name) = CString::new(name) else {
        return false;
    };
    !unsafe { libc::setlocale(category, name.as_ptr()) }.is_null()
}

fn compare(left: &(CString, String), right: &(CString, String)) -> Ordering {
    let order = unsafe { libc::strcoll(left.0.as_ptr(), right.0.as_ptr()) };
    order.cmp(&0).then_with(|| left.1.cmp(&right.1))
}

fn key(path: &str) -> CString {
    let bytes: Vec<u8> = crate::bytes::encode(path)
        .iter()
        .copied()
        .filter(|&byte| byte != 0)
        .collect();
    CString::new(bytes).unwrap_or_default()
}

pub fn sort(paths: &mut Vec<String>) {
    if BYTE_COLLATION.with(Cell::get) {
        paths.sort_by(|left, right| crate::bytes::encode(left).cmp(&crate::bytes::encode(right)));
        return;
    }
    let mut keyed: Vec<(CString, String)> =
        paths.drain(..).map(|path| (key(&path), path)).collect();
    keyed.sort_by(compare);
    paths.extend(keyed.into_iter().map(|(_, path)| path));
}

pub fn to_bytes_view(text: &str) -> String {
    crate::bytes::encode(text)
        .iter()
        .map(|&byte| crate::bytes::raw_char(byte))
        .collect()
}

pub fn from_bytes_view(text: &str) -> String {
    let bytes: Vec<u8> = text
        .chars()
        .map(|character| crate::bytes::raw_byte(character).unwrap_or(character as u8))
        .collect();
    crate::bytes::decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_order_names() {
        assert!(is_byte_order("C"));
        assert!(is_byte_order("POSIX"));
        assert!(is_byte_order("C.UTF-8"));
        assert!(!is_byte_order("en_US.UTF-8"));
    }

    #[test]
    fn byte_views_round_trip() {
        let view = to_bytes_view("aμb");
        assert_eq!(view.chars().count(), 4);
        assert_eq!(from_bytes_view(&view), "aμb");
        assert_eq!(from_bytes_view(&view[..1]), "a");
    }
}
