# Security policy

## Reporting a vulnerability

Please report security problems privately through [GitHub security advisories](https://github.com/gdprojectsss/goRustlyFast/security/advisories/new), not in a public issue. Include the grfsh version (`grfsh --version`), your operating system, and the smallest input or script that shows the problem.

You can expect an acknowledgement within a week. Fixes for confirmed issues ship in a patch release, and the advisory is published once the release is out.

## Supported versions

Security fixes go into the latest release. Older releases are not patched.

## What grfsh does to protect you

- **Startup files are checked before they run.** `~/.grfshrc`, `~/.grfsh_profile`, the files in `~/.config/grfsh/` and every plugin's `init.grfsh` are skipped with a warning when the file or its directory belongs to another user, or when other users can write to it. This keeps someone else on the machine from planting commands in your shell.
- **Private data stays private.** The history file, the `z` directory database and the command log in `~/.local/share/grfsh/commands.tsv` are written with mode `0600`.
- **Here documents never touch a shared path.** They are written to a temporary file created with `O_EXCL` and mode `0600`, and the file is unlinked before the command reads it.
- **`grfsh --check` never runs anything.** It only parses the script, so it is safe to point at scripts you do not trust yet.

## Things to keep in mind

- `source`, `eval`, plugins and `engine = starship` in a theme run code by design. Only use them with files and programs you trust.
- Completion rules added with `complete -C` run their command every time you press Tab on that command.
- A shell runs whatever you type with your privileges. grfsh does not sandbox commands.
