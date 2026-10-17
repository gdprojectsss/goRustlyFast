use crate::shell::{self, Flow, Shell};
use crate::sys;

fn source_options(args: &[String]) -> (Option<String>, &[String]) {
    match args {
        [flag, path, rest @ ..] if flag == "-p" => (Some(path.clone()), rest),
        [flag, rest @ ..] if flag == "--" => (None, rest),
        _ => (None, args),
    }
}

fn source_path(shell: &Shell, name: &str, search: Option<String>) -> String {
    if name.contains('/') || (!shell.options.sourcepath && search.is_none()) {
        return name.to_string();
    }
    let search = search
        .or_else(|| shell.get_variable("PATH"))
        .unwrap_or_default();
    search
        .split(':')
        .map(|directory| {
            let directory = if directory.is_empty() { "." } else { directory };
            format!("{directory}/{name}")
        })
        .find(|candidate| crate::bytes::to_path(candidate).is_file())
        .unwrap_or_else(|| name.to_string())
}

pub(super) fn source(shell: &mut Shell, command: &str, args: &[String]) -> i32 {
    let (search, args) = source_options(args);
    let Some(name) = args.first() else {
        shell::error(format!("{command}: filename argument required"));
        eprintln!("{command}: usage: {command} [-p path] filename [arguments]");
        return 2;
    };
    let path = &source_path(shell, name, search);
    if crate::bytes::to_path(path).is_dir() {
        shell::error(format!("{command}: {name}: is a directory"));
        return 1;
    }
    let mut reader = match crate::bytes::LineReader::open(path) {
        Ok(reader) => reader,
        Err(error) => {
            shell::error(format!("{name}: {}", sys::error_message(&error)));
            return 1;
        }
    };
    let contents = crate::bytes::read_file(path).unwrap_or_default();
    if let Some(hint) = crate::repl::startup_file_hint(path, &contents) {
        shell::error(hint);
    }

    let saved_positional =
        (args.len() > 1).then(|| std::mem::replace(&mut shell.positional, args[1..].to_vec()));
    shell.source_depth += 1;
    shell.call_frames.push((
        "source".to_string(),
        shell.current_line,
        shell.source_stack.last().cloned(),
    ));
    shell.source_stack.push(path.clone());
    let line = shell.current_line;
    let status = shell.run_lines(|| reader.next_line());
    if shell.flow == Flow::Return {
        shell.flow = Flow::Normal;
    }
    shell.run_named_trap("RETURN");
    shell.call_frames.pop();
    shell.current_line = line;
    shell.source_stack.pop();
    shell.source_depth -= 1;
    if let Some(positional) = saved_positional {
        shell.positional = positional;
    }
    status
}
