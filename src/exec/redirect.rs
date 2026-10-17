use crate::expand;
use crate::shell::{self, Shell};
use crate::syntax::ast::{Redirect, RedirectKind};
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
            shell::error(error);
            return 1;
        }
        let status = action(self);
        sys::restore_fds(saved);
        status
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
        let target = expand::single(self, &redirect.target)?;
        let open = |flags| {
            sys::open(&target, flags)
                .map_err(|error| format!("{target}: {}", sys::error_message(&error)))
        };
        let (source, owned) = match redirect.kind {
            RedirectKind::In => (open(libc::O_RDONLY)?, true),
            RedirectKind::Out | RedirectKind::OutAndError => {
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC)?, true)
            }
            RedirectKind::Append | RedirectKind::AppendAndError => {
                (open(libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND)?, true)
            }
            RedirectKind::ReadWrite => (open(libc::O_RDWR | libc::O_CREAT)?, true),
            RedirectKind::Heredoc => (temp(&crate::bytes::encode(&target))?, true),
            RedirectKind::HereString => {
                (temp(&crate::bytes::encode(&format!("{target}\n")))?, true)
            }
            RedirectKind::DuplicateIn | RedirectKind::DuplicateOut if target == "-" => {
                saved.push((redirect.fd, sys::save_fd(redirect.fd)));
                sys::close(redirect.fd);
                return Ok(());
            }
            RedirectKind::DuplicateIn | RedirectKind::DuplicateOut => {
                let fd = target
                    .parse()
                    .map_err(|_| format!("{target}: bad file descriptor"))?;
                (fd, false)
            }
        };

        if source != redirect.fd {
            saved.push((redirect.fd, sys::save_fd(redirect.fd)));
            let result = sys::dup2(source, redirect.fd);
            if owned {
                sys::close(source);
            }
            result.map_err(|error| format!("{target}: {}", sys::error_message(&error)))?;
        }
        if matches!(
            redirect.kind,
            RedirectKind::OutAndError | RedirectKind::AppendAndError
        ) {
            saved.push((2, sys::save_fd(2)));
            sys::dup2(redirect.fd, 2).map_err(|error| sys::error_message(&error))?;
        }
        Ok(())
    }
}

fn temp(content: &[u8]) -> Result<i32, String> {
    sys::temp_file_with(content)
        .map_err(|error| format!("here-document: {}", sys::error_message(&error)))
}
