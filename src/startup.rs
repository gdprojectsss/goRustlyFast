use crate::shell;
use crate::syntax::parser;

pub(crate) const DEFAULT_PATH: &str =
    "/usr/local/bin:/usr/local/sbin:/usr/bin:/usr/sbin:/bin:/sbin";

pub(crate) fn initialize_environment() {
    if std::env::var_os("PATH").is_none() {
        unsafe { std::env::set_var("PATH", DEFAULT_PATH) };
    }
    let level = next_shell_level(crate::bytes::env_var("SHLVL").as_deref());
    unsafe { std::env::set_var("SHLVL", level.to_string()) };
}

fn next_shell_level(current: Option<&str>) -> i64 {
    let current = current
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0);
    match current.saturating_add(1) {
        level if level < 0 => 0,
        level if level >= 1000 => 1,
        level => level,
    }
}

pub(crate) fn check_syntax(source: &str) -> i32 {
    match parser::parse(source) {
        Ok(_) => 0,
        Err(error) => {
            if let Some(line) = error.line {
                shell::set_location(None, line);
            }
            shell::error(&error.message);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_level_increments() {
        assert_eq!(next_shell_level(None), 1);
        assert_eq!(next_shell_level(Some("5")), 6);
        assert_eq!(next_shell_level(Some("abc")), 1);
        assert_eq!(next_shell_level(Some("-7")), 0);
        assert_eq!(next_shell_level(Some("999")), 1);
    }

    #[test]
    fn syntax_check_reports_errors() {
        assert_eq!(check_syntax("echo hi\nif true; then echo; fi\n"), 0);
        assert_eq!(check_syntax("if then\n"), 2);
    }
}
