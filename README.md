# who-me

`who-me` is a keyboard-first terminal dashboard for keeping the different parts of who you are in view. Create identity topics such as **Developer**, **Mountaineer**, or **Writer**, then keep a simple checklist inside each one.

The dashboard is deliberately small in scope: editable text, completion state, reordering, and search. It follows the active Omarchy color palette when one is available and falls back to a bundled true-color theme elsewhere.

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
| `↑` / `↓` | Move between a topic title and its entries |
| `←` / `→`, `Tab` | Move between topics |
| `t` | Add a topic |
| `a` | Add an entry to the selected topic |
| `Enter` | Edit the selected topic or entry |
| `Space` | Check or uncheck an entry |
| `Delete` or `Backspace` | Delete with confirmation |
| `Ctrl` + `↑` / `↓` | Reorder an entry |
| `Ctrl` + `←` / `→` | Reorder a topic |
| `/` | Search topics and entries |
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

[[topics.items]]
text = "Build thoughtful software"
done = true

[[topics.items]]
text = "Learn a new systems concept"
done = false
```

Writes use a temporary file and atomic rename. Before an existing file is replaced, its previous contents are copied to `data.toml.bak`. If the main file is malformed, `who-me` reports the exact problem and does not overwrite it.

On Omarchy, colors are read at startup from `$XDG_STATE_HOME/omarchy/current/theme/colors.toml`, or the corresponding path below `~/.local/state`.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

