pub(crate) const USAGE: &str = "usage: grfsh [options] [-c command [name [args...]]] [-s] [script [args...]]\n       grfsh --check script...\n       grfsh --explain command | --explain -f script\n       grfsh --serve socket | --client socket [-t seconds] command\noptions: -i -l -s -n, -abefhkmptuvxBCEHPT or -o option (+ turns off), -O shopt_option,\n         --login --norc --noprofile --rcfile file --posix --agent --json";

const SET_LETTERS: &str = "abefhkmptuvxBCEHPT";

#[derive(Debug, Default, PartialEq)]
pub(crate) enum Mode {
    #[default]
    Standard,
    Command(String),
    Script(String),
    Check(Vec<String>),
    Explain(Vec<String>),
    Serve(Vec<String>),
    Client(Vec<String>),
    Help,
    Version,
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Invocation {
    pub mode: Mode,
    pub login: bool,
    pub interactive: bool,
    pub agent: bool,
    pub json: bool,
    pub no_execute: bool,
    pub no_rc: bool,
    pub no_profile: bool,
    pub rc_file: Option<String>,
    pub set_arguments: Vec<String>,
    pub shopt_arguments: Vec<Vec<String>>,
    pub name: Option<String>,
    pub positional: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct UsageError {
    pub message: String,
    pub show_usage: bool,
}

impl UsageError {
    fn invalid(option: &str) -> Self {
        UsageError {
            message: format!("{option}: invalid option"),
            show_usage: true,
        }
    }

    fn missing_argument(option: &str) -> Self {
        UsageError {
            message: format!("{option}: option requires an argument"),
            show_usage: false,
        }
    }
}

struct Parser<'a> {
    args: &'a [String],
    index: usize,
    invocation: Invocation,
    command: bool,
    read_stdin: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Invocation, UsageError> {
    let mut parser = Parser {
        args,
        index: 0,
        invocation: Invocation::default(),
        command: false,
        read_stdin: false,
    };
    if parser.options()? {
        return Ok(parser.invocation);
    }
    parser.operands()?;
    Ok(parser.invocation)
}

impl Parser<'_> {
    fn next_argument(&mut self) -> Option<String> {
        let argument = self.args.get(self.index).cloned();
        if argument.is_some() {
            self.index += 1;
        }
        argument
    }

    fn rest(&self) -> Vec<String> {
        self.args[self.index.min(self.args.len())..].to_vec()
    }

    fn options(&mut self) -> Result<bool, UsageError> {
        while let Some(argument) = self.args.get(self.index).cloned() {
            if argument == "--" || argument == "-" {
                self.index += 1;
                return Ok(false);
            }
            let is_option = argument.len() > 1 && argument.starts_with(['-', '+']);
            if !is_option {
                return Ok(false);
            }
            self.index += 1;
            let finished = if argument.starts_with("--") {
                self.long_option(&argument)?
            } else {
                self.short_options(&argument)?
            };
            if finished {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn long_option(&mut self, argument: &str) -> Result<bool, UsageError> {
        let invocation = &mut self.invocation;
        match argument {
            "--login" => invocation.login = true,
            "--norc" => invocation.no_rc = true,
            "--noprofile" => invocation.no_profile = true,
            "--posix" | "--noediting" => {}
            "--verbose" => invocation.set_arguments.push("-v".into()),
            "--agent" => invocation.agent = true,
            "--json" => {
                invocation.agent = true;
                invocation.json = true;
            }
            "--rcfile" | "--init-file" => {
                let Some(file) = self.next_argument() else {
                    return Err(UsageError::missing_argument(&argument[2..]));
                };
                self.invocation.rc_file = Some(file);
            }
            "--help" => return self.finish(Mode::Help),
            "--version" => return self.finish(Mode::Version),
            "--check" => return self.finish(Mode::Check(self.rest())),
            "--explain" => return self.finish(Mode::Explain(self.rest())),
            "--serve" => {
                self.invocation.agent = true;
                return self.finish(Mode::Serve(self.rest()));
            }
            "--client" => return self.finish(Mode::Client(self.rest())),
            _ => return Err(UsageError::invalid(argument)),
        }
        Ok(false)
    }

    fn finish(&mut self, mode: Mode) -> Result<bool, UsageError> {
        self.invocation.mode = mode;
        Ok(true)
    }

    fn short_options(&mut self, argument: &str) -> Result<bool, UsageError> {
        match argument {
            "-h" => return self.finish(Mode::Help),
            "-V" => return self.finish(Mode::Version),
            _ => {}
        }
        let sign = &argument[..1];
        let enable = sign == "-";
        for letter in argument[1..].chars() {
            match letter {
                'c' if enable => self.command = true,
                'i' => self.invocation.interactive = enable,
                'l' => self.invocation.login = enable,
                's' => self.read_stdin = enable,
                'n' => self.invocation.no_execute = enable,
                'o' => self.named_option(sign),
                'O' => self.shopt_option(enable),
                letter if SET_LETTERS.contains(letter) => {
                    self.invocation
                        .set_arguments
                        .push(format!("{sign}{letter}"));
                }
                letter => return Err(UsageError::invalid(&format!("{sign}{letter}"))),
            }
        }
        Ok(false)
    }

    fn named_option(&mut self, sign: &str) {
        self.invocation.set_arguments.push(format!("{sign}o"));
        if let Some(name) = self.next_argument() {
            self.invocation.set_arguments.push(name);
        }
    }

    fn shopt_option(&mut self, enable: bool) {
        let mut arguments = vec!["shopt".to_string()];
        if let Some(name) = self.next_argument() {
            arguments.push(if enable { "-s" } else { "-u" }.to_string());
            arguments.push(name);
        }
        self.invocation.shopt_arguments.push(arguments);
    }

    fn operands(&mut self) -> Result<(), UsageError> {
        let mut rest = self.rest().into_iter();
        if self.command {
            let Some(source) = rest.next() else {
                return Err(UsageError::missing_argument("-c"));
            };
            self.invocation.mode = Mode::Command(source);
            self.invocation.name = rest.next();
        } else if !self.read_stdin
            && let Some(script) = rest.next()
        {
            self.invocation.mode = Mode::Script(script);
        }
        self.invocation.positional = rest.collect();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Invocation {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse(&args).expect("valid arguments")
    }

    fn rejected(args: &[&str]) -> UsageError {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse(&args).expect_err("invalid arguments")
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn clustered_login_and_command() {
        let invocation = parsed(&["-lc", "echo hi", "name", "one"]);
        assert!(invocation.login);
        assert_eq!(invocation.mode, Mode::Command("echo hi".into()));
        assert_eq!(invocation.name.as_deref(), Some("name"));
        assert_eq!(invocation.positional, strings(&["one"]));
        let invocation = parsed(&["-ilc", "env"]);
        assert!(invocation.interactive && invocation.login);
    }

    #[test]
    fn command_string_is_the_first_operand_after_all_options() {
        let invocation = parsed(&["-c", "-e", "echo hi"]);
        assert_eq!(invocation.mode, Mode::Command("echo hi".into()));
        assert_eq!(invocation.set_arguments, strings(&["-e"]));
    }

    #[test]
    fn set_options_and_named_options() {
        let invocation = parsed(&["-euxo", "pipefail", "+o", "vi", "-c", "true"]);
        assert_eq!(
            invocation.set_arguments,
            strings(&["-e", "-u", "-x", "-o", "pipefail", "+o", "vi"])
        );
        let invocation = parsed(&["-f", "-C", "-a", "-v", "script.sh"]);
        assert_eq!(invocation.set_arguments, strings(&["-f", "-C", "-a", "-v"]));
        assert_eq!(invocation.mode, Mode::Script("script.sh".into()));
    }

    #[test]
    fn shopt_options() {
        let invocation = parsed(&["-O", "nullglob", "+O", "dotglob"]);
        assert_eq!(
            invocation.shopt_arguments,
            vec![
                strings(&["shopt", "-s", "nullglob"]),
                strings(&["shopt", "-u", "dotglob"])
            ]
        );
    }

    #[test]
    fn stdin_with_arguments() {
        let invocation = parsed(&["-s", "a", "b"]);
        assert_eq!(invocation.mode, Mode::Standard);
        assert_eq!(invocation.positional, strings(&["a", "b"]));
    }

    #[test]
    fn end_of_options() {
        assert_eq!(parsed(&["--", "-c"]).mode, Mode::Script("-c".into()));
        assert_eq!(parsed(&["-", "x"]).mode, Mode::Script("x".into()));
        assert_eq!(parsed(&["-"]).mode, Mode::Standard);
    }

    #[test]
    fn long_options() {
        let invocation = parsed(&["--norc", "--noprofile", "--posix", "--rcfile", "rc", "-n"]);
        assert!(invocation.no_rc && invocation.no_profile && invocation.no_execute);
        assert_eq!(invocation.rc_file.as_deref(), Some("rc"));
        let invocation = parsed(&["--json", "--check", "a.sh"]);
        assert!(invocation.json && invocation.agent);
        assert_eq!(invocation.mode, Mode::Check(strings(&["a.sh"])));
    }

    #[test]
    fn errors() {
        assert_eq!(rejected(&["-Q"]), UsageError::invalid("-Q"));
        assert_eq!(rejected(&["--bogus"]), UsageError::invalid("--bogus"));
        assert!(!rejected(&["-c"]).show_usage);
        assert!(!rejected(&["--rcfile"]).show_usage);
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parsed(&["-h"]).mode, Mode::Help);
        assert_eq!(parsed(&["--version"]).mode, Mode::Version);
        assert_eq!(parsed(&["-eh"]).set_arguments, strings(&["-e", "-h"]));
    }
}
