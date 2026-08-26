# who-me

`who-me` is a keyboard-first terminal dashboard for keeping the different parts of who you are in view. Create identity topics such as **Developer**, **Mountaineer**, or **Writer**, then keep a simple checklist inside each one.

The dashboard is deliberately small in scope: editable text, completion state, identity lifecycle, reordering, and search. Mark identities as **Aspiring**, **Active**, or **Former** as they evolve. Each identity receives a stable symbol and accent derived from its name, while the selected card comes forward and the others stay calm. Cards adapt from one to four columns, and the whole identity map scrolls when it grows beyond the terminal.

`who-me` follows the complete active Omarchy palette when one is available and falls back to a bundled true-color theme elsewhere. Symbols and colors are generated at runtime, so the data file remains simple text with no presentation settings.

## Install

Rust is already included on a standard Omarchy installation. From this directory, run:

```sh
cargo install --path .
who-me
```

To try it without installing:

```sh
cargo run
```

## Controls

| Key | Action |
| --- | --- |
| `↑` / `↓` | Move through identity titles and entries |
| `←` / `→`, `Tab` | Move between topics |
| `t` | Add a topic |
| `a` | Add an entry to the selected topic |
| `s` | Choose the selected identity's status |
| `Enter` | Edit the selected topic or entry |
| `Space` | Check or uncheck an entry |
| `d` | Delete with confirmation |
| `Ctrl` + `↑` / `↓` | Reorder an entry |
| `Ctrl` + `←` / `→` | Reorder a topic |
| `/` | Search topics and entries |
| `g` | Open GitHub sync settings |
| `Esc` | Cancel, close, or clear search |
| `?` | Show the keyboard guide |
| `q` | Quit |

## Data

Data is saved after every confirmed change in:

```text
$XDG_DATA_HOME/who-me/data.toml
```

When `XDG_DATA_HOME` is unset, the path is `~/.local/share/who-me/data.toml`. The file is intentionally readable and can be edited while the app is closed:

```toml
version = 1

[[topics]]
name = "Developer"
status = "active"

[[topics.items]]
text = "Build thoughtful software"
done = true

[[topics.items]]
text = "Learn a new systems concept"
done = false
```

Identity statuses are `aspiring`, `active`, and `former`. Older files without a status remain valid and load as `active`.

Writes use a temporary file and atomic rename. Before an existing file is replaced, its previous contents are copied to `data.toml.bak`. If the main file is malformed, `who-me` reports the exact problem and does not overwrite it.

## Private GitHub Sync

Press `g` to connect a dedicated private GitHub repository. The repository must already exist, the `git` executable must be installed, and access must work through your normal Git credentials: an SSH key, SSH agent, or system Git credential helper. `who-me` accepts GitHub HTTPS and SSH URLs, disables interactive Git credential prompts, and never stores a token.

Sync commits use your normal Git author identity (`user.name` and `user.email`).

The local `data.toml` remains the working copy and is always saved first. A background worker synchronizes the repository's root `data.toml` after edits, at startup, and periodically while the app is open. If the network is unavailable, editing continues and pending changes retry automatically.

When both local and GitHub data changed and Git cannot merge them, the header shows **Conflict**. Open settings with `g` and choose either `l` to keep the current local document or `h` to use GitHub. Both versions are backed up under `$XDG_DATA_HOME/who-me/conflicts/` before resolution.

Disconnecting keeps the primary local file and archives the clone under `$XDG_DATA_HOME/who-me/sync-archives/`. Sync configuration is stored without credentials at `$XDG_CONFIG_HOME/who-me/config.toml` (or below `~/.config`).

On Omarchy, colors are read at startup from `$XDG_STATE_HOME/omarchy/current/theme/colors.toml`, or the corresponding path below `~/.local/state`.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
