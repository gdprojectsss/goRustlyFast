# Moving to grfsh, switching back and recovering

## Try it before you switch

Run `grfsh -l` from your current shell. It starts a login shell that reads `~/.grfsh_profile` and then `~/.config/grfsh/grfshrc` (or `~/.grfshrc`). Type `exit` to return to the shell you came from. Nothing changes until you run `chsh`.

Run `grfsh --check ~/.zshrc` (or any startup file you want to bring over) to see which lines grfsh cannot run, with a hint for each.

## Make it your login shell

```sh
command -v grfsh | sudo tee -a /etc/shells
chsh -s "$(command -v grfsh)"
```

Keep a terminal with your old shell open until a new terminal window starts grfsh correctly.

## Switching back

```sh
chsh -s /bin/zsh      # macOS default
chsh -s /bin/bash     # most Linux systems
```

`chsh` takes effect for new logins. If a terminal app runs grfsh directly instead of your login shell, change it there:

- macOS Terminal: Settings > General > "Shells open with": choose "Default login shell", or set the command back to `/bin/zsh`.
- iTerm2: Settings > Profiles > General > Command: "Login Shell".
- Ghostty: remove the `command = ...` line from `~/.config/ghostty/config`.
- VS Code: remove `terminal.integrated.defaultProfile.*` settings that point at grfsh.

## Recovering from a broken startup file

Errors and Ctrl-C in `~/.grfsh_profile` or the rc file do not end an interactive shell. If a startup file still keeps you out, for example because it runs `exit`, start grfsh without it:

```sh
grfsh -l --noprofile --norc
```

A command given to `ssh` or `grfsh -c` never reads startup files, so `ssh host 'mv ~/.grfsh_profile ~/.grfsh_profile.broken'` works from another machine. You can also run `chsh -s /bin/zsh` from another terminal, or set macOS Terminal's "Shells open with" to `/bin/zsh`.
