fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn event<'a>(history: &'a [String], designator: &str) -> Result<&'a str, String> {
    let not_found = || format!("!{designator}: event not found");
    let found = if designator == "!" {
        history.last()
    } else if let Ok(number) = designator.parse::<i64>() {
        if number < 0 {
            history
                .len()
                .checked_sub(number.unsigned_abs() as usize)
                .and_then(|index| history.get(index))
        } else {
            (number as usize)
                .checked_sub(1)
                .and_then(|index| history.get(index))
        }
    } else {
        history
            .iter()
            .rev()
            .find(|line| line.starts_with(designator))
    };
    found.map(String::as_str).ok_or_else(not_found)
}

pub(super) fn expand(line: &str, history: &[String]) -> Result<Option<String>, String> {
    if !line.contains('!') {
        return Ok(None);
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut changed = false;
    let mut in_single = false;
    let mut index = 0;

    while index < chars.len() {
        let character = chars[index];
        if character == '\'' {
            in_single = !in_single;
        }
        if character == '\\' && index + 1 < chars.len() {
            out.push(character);
            out.push(chars[index + 1]);
            index += 2;
            continue;
        }
        if character != '!' || in_single {
            out.push(character);
            index += 1;
            continue;
        }

        let next = chars.get(index + 1).copied();
        let replacement = match next {
            Some('!') => {
                index += 2;
                event(history, "!")?.to_string()
            }
            Some('$') => {
                index += 2;
                let last = event(history, "!")?;
                words(last)
                    .last()
                    .map(|word| word.to_string())
                    .unwrap_or_default()
            }
            Some('^') => {
                index += 2;
                let last = event(history, "!")?;
                words(last)
                    .get(1)
                    .map(|word| word.to_string())
                    .unwrap_or_default()
            }
            Some('*') => {
                index += 2;
                let last = event(history, "!")?;
                words(last)
                    .iter()
                    .skip(1)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            Some(start) if start == '-' || start.is_ascii_alphanumeric() => {
                let begin = index + 1;
                let mut end = begin + 1;
                let numeric = start == '-' || start.is_ascii_digit();
                while let Some(&following) = chars.get(end) {
                    let continues = if numeric {
                        following.is_ascii_digit()
                    } else {
                        !following.is_whitespace() && !";|&()<>'\"".contains(following)
                    };
                    if !continues {
                        break;
                    }
                    end += 1;
                }
                let designator: String = chars[begin..end].iter().collect();
                if designator == "-" {
                    out.push('!');
                    index += 1;
                    continue;
                }
                index = end;
                event(history, &designator)?.to_string()
            }
            _ => {
                out.push('!');
                index += 1;
                continue;
            }
        };
        out.push_str(&replacement);
        changed = true;
    }

    Ok(changed.then_some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<String> {
        vec![
            "git status".into(),
            "cargo build --release".into(),
            "ls -la src".into(),
        ]
    }

    #[test]
    fn previous_command_and_words() {
        let history = history();
        assert_eq!(
            expand("sudo !!", &history).unwrap().as_deref(),
            Some("sudo ls -la src")
        );
        assert_eq!(
            expand("cd !$", &history).unwrap().as_deref(),
            Some("cd src")
        );
        assert_eq!(
            expand("echo !^", &history).unwrap().as_deref(),
            Some("echo -la")
        );
        assert_eq!(
            expand("echo !*", &history).unwrap().as_deref(),
            Some("echo -la src")
        );
    }

    #[test]
    fn events_by_number_and_prefix() {
        let history = history();
        assert_eq!(
            expand("!1", &history).unwrap().as_deref(),
            Some("git status")
        );
        assert_eq!(
            expand("!-2", &history).unwrap().as_deref(),
            Some("cargo build --release")
        );
        assert_eq!(
            expand("!car", &history).unwrap().as_deref(),
            Some("cargo build --release")
        );
        assert!(expand("!nothing", &history).is_err());
    }

    #[test]
    fn literal_bangs_stay() {
        let history = history();
        assert_eq!(expand("echo hi", &history).unwrap(), None);
        assert_eq!(expand("echo '!!' done!", &history).unwrap(), None);
        assert_eq!(expand("[ ! -d x ]", &history).unwrap(), None);
        assert_eq!(expand("echo \\!!", &history).unwrap(), None);
    }
}
