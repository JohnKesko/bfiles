# bfiles

A very fast directory size analyzer written in Rust.   
Provide a path to find which folders eating up the most space.

Uses concurrent traversal (crossbeam or rayon) to scan directories quickly.

## Usage

```
bfiles --path <PATH> [OPTIONS]
```

### Options

- `--path <PATH>` — Path to analyze (required)
- `--engine <ENGINE>` — `rayon` or `crossbeam` (default: crossbeam)
- `--max_depth <N>` — Limit traversal depth (default: unlimited)
- `--top <N>` — Show top N largest directories (default: 10)
- `-h, --help` — Show help

### Examples

```
bfiles --path .
bfiles --path /Users --max_depth 2 --top 20
bfiles --path . --engine rayon
```

## Building

```
cargo build --release
```

The binary ends up in `target/release/bfiles`.

## License

MIT
