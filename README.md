# bfiles

bfiles (big files) is a fast directory size analyzer written in Rust.
Point it at a folder and it shows you which folders use the most disk space.

## Installation

### macOS and Linux (including Raspberry Pi)

```
curl -fsSL https://raw.githubusercontent.com/johnkesko/bfiles/master/install.sh | sh
```

### Windows

Run in PowerShell:

```
irm https://raw.githubusercontent.com/johnkesko/bfiles/master/install.ps1 | iex
```

### Manual download

Prebuilt binaries are available for:

* Linux x86_64
* Linux aarch64 (Raspberry Pi 3/4/5 and other ARM64 boards, static binary)
* macOS arm64
* macOS x86_64
* Windows x86_64

To update later, run `bfiles upgrade` — it replaces the binary in place with the latest release.

## Usage

```
bfiles -p <PATH> [OPTIONS]
bfiles upgrade
```

By default you get a compact table of the largest folders and a total.
Add `-d` to also see what is inside each of them.

```
bfiles -p . -t 3

Traversed 2063 items in 15.33ms

Folder        Size
target   228.05 MB
src       21.86 KB
.git      13.69 KB
Total    228.09 MB
```

```
bfiles -p . -t 3 -d

Traversed 2063 items in 15.33ms

Folder        Size
target   228.05 MB
src       21.86 KB
.git      13.69 KB
Total    228.09 MB

Details

target                     228.05 MB
- target/debug             125.29 MB
- target/release           102.69 MB
- target/release/deps       90.07 MB
- target/debug/deps         77.79 MB
- target/debug/incremental  33.44 MB

src                         21.86 KB
- src/traverse               6.17 KB

.git                        13.69 KB
- .git/objects              10.54 KB
- .git/objects/16            2.95 KB
- .git/objects/29            1.33 KB
- .git/objects/70            1.20 KB
- .git/logs                 990.00 B
```

The table has one row per folder directly under the scanned path. Files that
sit directly in the scanned path are grouped under `(files)`. In the details
view, each folder lists up to 5 of its largest subfolders.

## Options

* `-p, --path <PATH>` — Path to scan; `user@host:/path` scans a remote machine over ssh
* `-d, --details` — Also show what is inside each folder
* `-t, --top <N>` — How many folders to show (default: 10)
* `-m, --max_depth <N>` — How deep to scan (default: no limit)
* `-e, --engine <ENGINE>` — `rayon` or `crossbeam` (default: crossbeam)
* `--exclude <PATHS>` — Skip folders; separate several with `|`, or repeat the flag
* `--include-cloud` — Also scan cloud-synced folders (skipped by default, see below)
* `-h, --help` — Show help
* `-V, --version` — Show version

More examples:

```
bfiles -p ~ --exclude "~/Library|~/.cache"
bfiles -p . -e rayon -t 20
bfiles -p 'pi@pi01:/srv/storage' -t 10
```

A leading `~` in paths works even inside quotes — bfiles expands it itself.

## Remote scanning

Give `-p` an address in the same form scp uses, and the scan runs on that
machine over ssh:

```
bfiles -p 'user@host:/srv/storage' -t 10
```

The remote machine scans its own disks at local speed and sends back only the
final summary — a few kilobytes. This is much faster than scanning a network
mount of the same data, where every folder listing is a network round-trip.

Requirements: bfiles v0.4 or newer must be installed on the remote machine,
and `ssh user@host` must work (keys, agents, and password prompts behave
exactly as with plain ssh). The scan flags `-t`, `-m`, `-e`, `--exclude`, and
`--include-cloud` apply on the remote side; `-d` changes how the result is
shown locally.

## How sizes are measured

* Sizes are logical file sizes (what `ls -l` shows), not disk blocks. Sparse files count at their full size, so numbers match Finder rather than `du`.
* Hardlinked files count once per location, like Finder. `du` counts them once in total, so bfiles can report more than `du` on trees with many hardlinks.
* Symlinks are never followed and count as zero bytes.
* `-m, --max_depth` limits how deep the scan goes. Anything below the limit is not measured at all, so the sizes shown are partial. Run without it to get true totals.
* Folders that cannot be read (missing permissions) are skipped and reported in a warning; totals are then underestimates.
* Cloud-synced folders (`~/Library/CloudStorage` and `~/Library/Mobile Documents` on macOS) are skipped by default with a notice, because listing them goes through each provider's sync service and can take minutes. Pass `--include-cloud` to scan them anyway, or point bfiles directly at one of them.

## Platform support

bfiles works on macOS, Linux, and Windows. The test suite runs on all three in CI.

* macOS uses decimal size units (`KB = 1000`)
* Windows and Linux use binary size units (`KB = 1024`)

## Building from source

```
cargo build --release
```

The binary is created at `target/release/bfiles`. You can also install it with Cargo:

```
cargo install --path . --force
```

## License

MIT
