use crate::shell::{self, Shell};

fn finish_word(shell: &mut Shell, index: usize) {
    shell.getopts_offset = 0;
    shell.set_variable("OPTIND", &(index + 2).to_string());
}

pub(super) fn run(shell: &mut Shell, args: &[String]) -> i32 {
    let [specification, name, rest @ ..] = args else {
        shell::error("getopts: usage: getopts optstring name [arg ...]");
        return 2;
    };
    let words: Vec<String> = if rest.is_empty() {
        shell.positional.clone()
    } else {
        rest.to_vec()
    };
    let silent = specification.starts_with(':');
    let specification = specification.trim_start_matches(':');

    let option_index: usize = shell
        .get_variable("OPTIND")
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value >= 1)
        .unwrap_or(1);
    if shell.getopts_index != option_index {
        shell.getopts_offset = 0;
    }
    let index = option_index - 1;

    let Some(word) = words.get(index) else {
        shell.set_variable(name, "?");
        return 1;
    };
    if shell.getopts_offset == 0 {
        if word == "--" {
            shell.set_variable("OPTIND", &(index + 2).to_string());
            shell.getopts_index = index + 2;
            shell.set_variable(name, "?");
            return 1;
        }
        if !word.starts_with('-') || word == "-" {
            shell.set_variable(name, "?");
            return 1;
        }
        shell.getopts_offset = 1;
    }

    let characters: Vec<char> = word.chars().collect();
    let option = characters[shell.getopts_offset];
    shell.getopts_offset += 1;
    let at_end = shell.getopts_offset >= characters.len();
    let position = specification.find(option);
    let takes_argument = position.is_some_and(|found| specification[found + 1..].starts_with(':'));

    if position.is_none() || option == ':' {
        if silent {
            shell.set_variable("OPTARG", &option.to_string());
        } else {
            shell::error(format!("getopts: illegal option -- {option}"));
            shell.unset_variable("OPTARG");
        }
        shell.set_variable(name, "?");
        if at_end {
            finish_word(shell, index);
        }
    } else if takes_argument {
        if !at_end {
            let value: String = characters[shell.getopts_offset..].iter().collect();
            shell.set_variable("OPTARG", &value);
            finish_word(shell, index);
            shell.set_variable(name, &option.to_string());
        } else if let Some(value) = words.get(index + 1) {
            shell.set_variable("OPTARG", value);
            shell.getopts_offset = 0;
            shell.set_variable("OPTIND", &(index + 3).to_string());
            shell.set_variable(name, &option.to_string());
        } else {
            finish_word(shell, index);
            if silent {
                shell.set_variable("OPTARG", &option.to_string());
                shell.set_variable(name, ":");
            } else {
                shell::error(format!("getopts: option requires an argument -- {option}"));
                shell.unset_variable("OPTARG");
                shell.set_variable(name, "?");
            }
        }
    } else {
        shell.unset_variable("OPTARG");
        shell.set_variable(name, &option.to_string());
        if at_end {
            finish_word(shell, index);
        }
    }
    shell.getopts_index = shell
        .get_variable("OPTIND")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    0
}
