# bfiles

bfiles (big files) is a fast directory size analyzer written in Rust.
Provide a path to find which folders are using the most disk space.

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

Prebuilt binaries are available:

* Linux x86_64
* Linux aarch64 (Raspberry Pi 3/4/5 and other ARM64 boards, static binary)
* macOS arm64
* macOS x86_64
* Windows x86_64

## Platform Support

The app is intended to work on macOS, Linux, and Windows.

* macOS uses decimal size units (`KB = 1000`)
* Windows and Linux use binary size units (`KB = 1024`)
* CI runs the test suite on all three operating systems

## Remote scanning

Point bfiles at an scp-style address and the scan runs on that machine over ssh:

```
bfiles -p 'user@host:/srv/storage' -t 10
```

The remote host does the entire scan and aggregation natively — only a tiny
summary crosses the network, so scanning a NAS this way is orders of magnitude
faster than scanning an SMB/NFS mount of it. Requires bfiles (v0.4+) on the
remote host; ssh keys, agents, and password prompts work as they do for plain
`ssh`. All flags (`-t`, `-d`, `-e`, `--exclude`, `--include-cloud`) apply on
the remote side.

## How sizes are measured

* Sizes are logical file sizes (what `ls -l` shows), not disk blocks. Sparse files count at their full logical size, so numbers match Finder rather than `du`.
* Hardlinked files count once per location, like Finder. `du` counts them once in total, so bfiles can report more than `du` on trees with many hardlinks.
* Symlinks are never followed and count as zero bytes.
* `-d, --max_depth` limits how deep the scan goes. Anything below the limit is not measured at all, so the sizes shown are partial. Run without `-d` to get true totals.
* Directories that cannot be read (permissions) are skipped and reported in a warning; totals are then underestimates.
* Cloud-synced folders (`~/Library/CloudStorage` and `~/Library/Mobile Documents` on macOS) are skipped by default with a notice. Listing them goes through each provider's daemon instead of the disk and can take minutes. Pass `--include-cloud` to scan them anyway, or point bfiles directly at one of them.
* Files directly in the scanned root are shown as a `(files)` group, and a `Total` row sums everything that was measured.

## Usage

```
bfiles --path <PATH> [OPTIONS]
bfiles -p <PATH> [OPTIONS]
bfiles upgrade            # update to the latest release in place
```

## Options

* `-p, --path <PATH>` — Path to analyze; `user@host:/path` scans a remote host over ssh
* `-e, --engine <ENGINE>` — `rayon` or `crossbeam` (default: crossbeam)
* `-d, --max_depth <N>` — Limit traversal depth
* `-t, --top <N>` — Show top N root groups
* `--exclude <PATH>` — Exclude a directory from the scan (repeatable)
* `--include-cloud` — Also scan cloud-synced folders (skipped by default, see below)
* `-h, --help` — Show help
* `-V, --version` — Show version

Output is grouped by the immediate child directories under the scanned root path. Each group shows up to 5 of its largest descendant directories.

## Examples

```
bfiles -p .
bfiles -p . -t 10
bfiles --path ./my-folder --max_depth 2 --top 20
bfiles --path . --engine rayon
```

Example output:

```
bfiles -p . -t 3

Traversed 2063 items in 15.33ms

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

## Building from source

```
cargo build --release
```

The binary is created at:

```
target/release/bfiles
```

You can also install it locally with Cargo:

```
cargo install --path . --force
```

## License

MIT
