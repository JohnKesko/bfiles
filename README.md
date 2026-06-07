# bfiles

bfiles (big files) is a fast directory size analyzer written in Rust.
Provide a path to find which folders are using the most disk space.

## Installation

### macOS and Linux

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
* macOS arm64
* macOS x86_64
* Windows x86_64

## Platform Support

The app is intended to work on macOS, Linux, and Windows.

* macOS uses decimal size units (`KB = 1000`)
* Windows and Linux use binary size units (`KB = 1024`)
* CI runs the test suite on all three operating systems

## Usage

```
bfiles --path <PATH> [OPTIONS]
bfiles -p <PATH> [OPTIONS]
bfiles upgrade            # update to the latest release in place
```

## Options

* `-p, --path <PATH>` — Path to analyze
* `-e, --engine <ENGINE>` — `rayon` or `crossbeam` (default: crossbeam)
* `-d, --max_depth <N>` — Limit traversal depth
* `-t, --top <N>` — Show top N root groups
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
