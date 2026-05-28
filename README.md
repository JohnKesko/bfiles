# bfiles

bfiles (big files) is a very fast directory size analyzer written in Rust.   
Provide a path to find which folders eating up the most space.

Uses concurrent traversal (crossbeam or rayon) to scan directories quickly.

## Downloads

Tagged releases publish prebuilt binaries on the GitHub Releases page for:

- Linux x86_64
- macOS arm64
- macOS x86_64
- Windows x86_64

Push a tag like `v0.1.0` to trigger the release workflow.

## Platform Support

The app is intended to work on macOS, Linux, and Windows.

- macOS uses decimal size units (`KB = 1000`)
- Windows and Linux use binary size units (`KB = 1024`)
- CI runs the test suite on all three operating systems

## Usage

```
bfiles --path <PATH> [OPTIONS]
bfiles -p <PATH> [OPTIONS]
```

### Options

- `-p, --path <PATH>` — Path to analyze (required)
- `-e, --engine <ENGINE>` — `rayon` or `crossbeam` (default: crossbeam)
- `-d, --max_depth <N>` — Limit traversal depth (default: unlimited)
- `-t, --top <N>` — Show top N root groups (default: 10)
- `-h, --help` — Show help

Output is grouped by the immediate child directories under the scanned root path, and each group shows up to 5 of its largest descendant directories.

### Examples

```
bfiles -p .
bfiles -p . -t 10
bfiles --path ./my-folder --max_depth 2 --top 20
bfiles --path . --engine rayon
```

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

## Building

```
cargo build --release
```

The binary ends up in `target/release/bfiles`.

## License

MIT
