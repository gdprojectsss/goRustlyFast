use crate::builtins::deparse;
use crate::expand;
use crate::shell::{self, Shell};
use crate::syntax::ast::{Redirect, RedirectKind, Word};
use crate::syntax::lexer;
use crate::sys;

impl Shell {
    pub(crate) fn with_redirects(
        &mut self,
        redirects: &[Redirect],
        action: impl FnOnce(&mut Self) -> i32,
    ) -> i32 {
        if redirects.is_empty() {
            return action(self);
        }
        let mut saved = Vec::new();
        if let Err(error) = self.redirect_saving(redirects, &mut saved) {
            sys::restore_fds(saved);
            return self.redirect_failed(error);
        }
        let status = action(self);
        sys::restore_fds(saved);
        status
    }

    pub(crate) fn redirect_failed(&mut self, error: String) -> i32 {
        if error.starts_with(expand::NO_MATCH) {
            self.flow = shell::Flow::Interrupted;
        }
        shell::error(error);
        1
    }

    pub(crate) fn apply_redirects(&mut self, redirects: &[Redirect]) -> Result<(), String> {
        let mut saved = Vec::new();
        self.redirect_saving(redirects, &mut saved)?;
        for (_, copy) in saved {
            if let Some(fd) = copy {
                sys::close(fd);
            }
        }
        Ok(())
    }

    pub(crate) fn redirect_saving(
        &mut self,
        redirects: &[Redirect],
        saved: &mut Vec<(i32, Option<i32>)>,
    ) -> Result<(), String> {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        for redirect in redirects {
            self.redirect_one(redirect, saved)?;
        }
        Ok(())
    }

    pub(crate) fn redirect_one(
        &mut self,
        redirect: &Redirect,
        saved: &mut Vec<(i32, Option<i32>)>,
    ) -> Result<(), String> {
        if let Some(variable) = &redirect.variable {
            return self.redirect_named(variable, redirect);
        }
        let target = self.redirect_target(redirect)?;
        let mut kind = redirect.kind;
        if matches!(kind, RedirectKind::DuplicateIn | RedirectKind::DuplicateOut) {
            match duplicate_target(redirect, &target)? {
                Duplicate::Close => {
                    saved.push((redirect.fd, sys::save_fd(redirect.fd)));
                    sys::close(redirect.fd);
                    return Ok(());
                }
                Duplicate::Descriptor(fd, move_source) => {
                    return self.duplicate(redirect, fd, move_source, saved);
                }
                Duplicate::File => kind = RedirectKind::OutAndError,
            }
        }
        let open = |flags| {
            sys::open(&target, flags)
                .map_err(|error| format!("{target}: {}", sys::error_message(&error)))
        };
        let (source, owned) = match kind {
            RedirectKind::In => (open(libc::O_RDONLY)?, true),
            RedirectKind::Out | RedirectKind::OutAndError | RedirectKind::Clobber => {
                if kind != RedirectKind::Clobber
                    && self.options.noclobber
                    && std::path::Path::new(&target).is_file()
                {
                    return Err(format!("{target}: cannot overwrite existing file"));
                }
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC)?, true)
            }
            RedirectKind::Append | RedirectKind::AppendAndError => {
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND)?, true)
            }
            RedirectKind::ReadWrite => (open(libc::O_RDWR | libc::O_CREAT)?, true),
            RedirectKind::Heredoc => (here_document(&crate::bytes::encode(&target))?, true),
            RedirectKind::HereString => (
                here_document(&crate::bytes::encode(&format!("{target}\n")))?,
                true,
            ),
            RedirectKind::DuplicateIn | RedirectKind::DuplicateOut => {
                return Err(format!("{target}: ambiguous redirect"));
            }
        };

        if source != redirect.fd {
            saved.push((redirect.fd, sys::save_fd(redirect.fd)));
            let result = sys::dup2(source, redirect.fd);
            if owned {
                sys::close(source);
            }
            result.map_err(|error| format!("{target}: {}", sys::error_message(&error)))?;
        } else if owned {
            saved.push((redirect.fd, None));
            sys::clear_cloexec(redirect.fd);
        }
        if matches!(
            kind,
            RedirectKind::OutAndError | RedirectKind::AppendAndError
        ) {
            saved.push((2, sys::save_fd(2)));
            sys::dup2(redirect.fd, 2).map_err(|error| sys::error_message(&error))?;
        }
        Ok(())
    }
}

enum Duplicate {
    Close,
    Descriptor(i32, bool),
    File,
}

fn duplicate_target(redirect: &Redirect, target: &str) -> Result<Duplicate, String> {
    if target == "-" {
        return Ok(Duplicate::Close);
    }
    let (number, move_source) = match target.strip_suffix('-') {
        Some(number) => (number, true),
        None => (target, false),
    };
    if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
        return match number.parse::<i32>() {
            Ok(fd) => Ok(Duplicate::Descriptor(fd, move_source)),
            Err(_) => Err(bad_descriptor(redirect)),
        };
    }
    if redirect.kind == RedirectKind::DuplicateOut && redirect.fd == 1 {
        return Ok(Duplicate::File);
    }
    Err(format!(
        "{}: ambiguous redirect",
        deparse::word(&redirect.target)
    ))
}

fn bad_descriptor(redirect: &Redirect) -> String {
    let word = deparse::word(&redirect.target);
    format!("{}: Bad file descriptor", word.trim_end_matches('-'))
}

const PIPE_LIMIT: usize = 4096;

fn here_document(content: &[u8]) -> Result<i32, String> {
    if content.len() > PIPE_LIMIT {
        return temp(content);
    }
    let failed = |error: std::io::Error| format!("here-document: {}", sys::error_message(&error));
    let (read_end, write_end) = sys::pipe().map_err(failed)?;
    let written = sys::write_all(write_end, content);
    sys::close(write_end);
    if let Err(error) = written {
        sys::close(read_end);
        return Err(failed(error));
    }
    Ok(read_end)
}

impl Shell {
    fn duplicate(
        &mut self,
        redirect: &Redirect,
        source: i32,
        move_source: bool,
        saved: &mut Vec<(i32, Option<i32>)>,
    ) -> Result<(), String> {
        if unsafe { libc::fcntl(source, libc::F_GETFD) } < 0 {
            return Err(bad_descriptor(redirect));
        }
        if source == redirect.fd {
            return Ok(());
        }
        saved.push((redirect.fd, sys::save_fd(redirect.fd)));
        sys::dup2(source, redirect.fd).map_err(|_| bad_descriptor(redirect))?;
        if move_source {
            sys::close(source);
        }
        Ok(())
    }

    fn named_fd_word(variable: &str) -> Result<Word, String> {
        lexer::parse_double_quoted_text(&format!("${{{variable}}}")).map_err(|error| error.message)
    }

    fn redirect_target(&mut self, redirect: &Redirect) -> Result<String, String> {
        if matches!(
            redirect.kind,
            RedirectKind::Heredoc | RedirectKind::HereString
        ) {
            return expand::single(self, &redirect.target);
        }
        let fields = expand::words(self, std::slice::from_ref(&redirect.target))?;
        match <[String; 1]>::try_from(fields) {
            Ok([target]) => Ok(target),
            Err(_) => Err(format!(
                "{}: ambiguous redirect",
                deparse::word(&redirect.target)
            )),
        }
    }

    fn redirect_named(&mut self, variable: &str, redirect: &Redirect) -> Result<(), String> {
        let target = self.redirect_target(redirect)?;
        let duplicate = matches!(
            redirect.kind,
            RedirectKind::DuplicateIn | RedirectKind::DuplicateOut
        );
        if duplicate && target == "-" {
            let value = expand::single(self, &Self::named_fd_word(variable)?)?;
            let fd: i32 = value
                .trim()
                .parse()
                .map_err(|_| format!("{variable}: ambiguous redirect"))?;
            sys::close(fd);
            return Ok(());
        }
        let open = |flags| {
            sys::open(&target, flags)
                .map_err(|error| format!("{target}: {}", sys::error_message(&error)))
        };
        let (source, owned) = match redirect.kind {
            RedirectKind::In => (open(libc::O_RDONLY)?, true),
            RedirectKind::Out | RedirectKind::OutAndError | RedirectKind::Clobber => {
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC)?, true)
            }
            RedirectKind::Append | RedirectKind::AppendAndError => {
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND)?, true)
            }
            RedirectKind::ReadWrite => (open(libc::O_RDWR | libc::O_CREAT)?, true),
            RedirectKind::Heredoc => (here_document(&crate::bytes::encode(&target))?, true),
            RedirectKind::HereString => (
                here_document(&crate::bytes::encode(&format!("{target}\n")))?,
                true,
            ),
            RedirectKind::DuplicateIn | RedirectKind::DuplicateOut => {
                match duplicate_target(redirect, &target)? {
                    Duplicate::Descriptor(fd, _) => (fd, false),
                    _ => return Err(format!("{target}: ambiguous redirect")),
                }
            }
        };
        let fd = unsafe { libc::fcntl(source, libc::F_DUPFD, 10) };
        if owned {
            sys::close(source);
        }
        if fd < 0 {
            return Err(bad_descriptor(redirect));
        }
        self.assign_named_fd(variable, fd)
    }

    fn assign_named_fd(&mut self, variable: &str, fd: i32) -> Result<(), String> {
        let Some((name, index)) = variable
            .strip_suffix(']')
            .and_then(|rest| rest.split_once('['))
        else {
            return self.try_set_variable(variable, &fd.to_string());
        };
        let index = lexer::parse_double_quoted_text(index).map_err(|error| error.message)?;
        let key = self.array_key(name, &index)?;
        self.set_element(name, key, &fd.to_string())
    }
}

fn temp(content: &[u8]) -> Result<i32, String> {
    sys::temp_file_with(content)
        .map_err(|error| format!("here-document: {}", sys::error_message(&error)))
}
