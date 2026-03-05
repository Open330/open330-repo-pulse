<p align="center">
  <img src="assets/logo.svg" width="128" alt="Open330 Repo Pulse Logo"/>
</p>

<h1 align="center">Open330 Repo Pulse</h1>

<p align="center">
  <strong>Scan GitHub organization repositories and detect maintenance risk in seconds</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust" alt="Rust 2024">
  <img src="https://img.shields.io/badge/Reqwest-0.12-5A29E4" alt="Reqwest">
  <img src="https://img.shields.io/badge/Clap-4-1E90FF" alt="Clap">
  <img src="https://img.shields.io/badge/Serde-1-DD6B20" alt="Serde">
  <img src="https://img.shields.io/badge/License-MIT-22C55E" alt="MIT License">
</p>

<p align="center">
  <a href="#quick-start"><strong>Quick Start -></strong></a>
</p>

<p align="center">
  Repo Pulse fetches repositories from a GitHub organization, scores each repo's maintenance health, and outputs actionable reports in table, Markdown, or JSON format.
</p>

---

<div><img src="https://quickstart-for-agents.vercel.app/api/header.svg?theme=opencode&logo=github&title=Scan+GitHub+org+repos+for+maintenance+risk+with+Rust&font=inter" width="100%" /></div>

```
You are an AI agent working on open330-repo-pulse, a Rust 2024 CLI that
scans GitHub organization repositories, scores maintenance health, and emits
table/Markdown/JSON reports for triage and automation workflows.
Clone https://github.com/Open330/open330-repo-pulse and help improve scoring,
add output integrations, or tune risk heuristics for large organizations.
```

## Features

**Organization Scan** -- Pull repositories from any GitHub organization through the REST API with optional private repository support via `--include-private` + `GITHUB_TOKEN`.

**Risk Scoring** -- Calculate health scores (`0-100`) using activity recency, repository metadata quality, archive status, and engagement signal.

**Status Buckets** -- Classify each repository as `healthy`, `watch`, or `stale` so maintainers can prioritize action quickly.

**Multi-Format Output** -- Render reports as readable terminal tables, Markdown for issues/docs, or JSON for automation pipelines.

**Richer Context Columns** -- Include default branch and fork count in tabular output, while `notes` flags key context such as archived/private repositories and metadata gaps.

**Deterministic Sorting** -- Sort by health, update recency, or repository name depending on the workflow.

**Safe Defaults** -- Defaults tuned for org-level triage: `--org open330`, `--max-repos 100`, `--stale-days 45` (inclusive stale boundary).

## Supported Formats

| Format | Use Case |
| --- | --- |
| `table` | Human-friendly terminal triage output |
| `markdown` | Paste into issues, PRs, and docs |
| `json` | Feed CI jobs and automation tooling |

## Sort Modes

| Mode | Behavior |
| --- | --- |
| `health` | Prioritize at-risk repositories first |
| `updated` | Show most recently pushed repositories first |
| `name` | Stable alphabetical ordering |

## CLI Options

| Option | Default | Description |
| --- | --- | --- |
| `--org` | `open330` | Target GitHub organization |
| `--max-repos` | `100` | Maximum repositories to fetch |
| `--stale-days` | `45` | Inclusive stale threshold in days (`>=` is stale) |
| `--format` | `table` | Output format: `table`, `markdown`, `json` |
| `--sort` | `health` | Sort mode: `health`, `updated`, `name` |
| `--include-private` | `false` | Include private repositories (requires non-empty `GITHUB_TOKEN`) |

## Export Presets

| Preset | Command |
| --- | --- |
| Default org triage | `cargo run --` |
| Weekly markdown report | `cargo run -- --format markdown --sort health` |
| Automation JSON feed | `cargo run -- --format json --sort updated` |
| Multi-org scan | `cargo run -- --org rust-lang --max-repos 80 --stale-days 60` |

## Keyboard Shortcuts

| Action | Shortcut |
| --- | --- |
| Repeat previous command | `↑` + `Enter` |
| Search command history | `Ctrl+R` |
| Interrupt current scan | `Ctrl+C` |
| Clear terminal | `Ctrl+L` |

## Architecture

```text
open330-repo-pulse/
├── assets/
│   └── logo.svg               # README logo
├── src/
│   ├── main.rs                # CLI argument parsing and execution flow
│   ├── lib.rs                 # Module exports
│   ├── github.rs              # GitHub REST client + pagination
│   ├── report.rs              # Health scoring, classification, sorting, tests
│   └── output.rs              # Table/Markdown/JSON rendering
└── Cargo.toml                 # Rust dependencies and package metadata
```

## Tech Stack

| Layer | Technology |
| --- | --- |
| Language | Rust (edition 2024) |
| CLI | [clap](https://github.com/clap-rs/clap) |
| HTTP | [reqwest](https://github.com/seanmonstar/reqwest) (blocking + rustls) |
| Serialization | [serde](https://serde.rs/) + [serde_json](https://github.com/serde-rs/json) |
| Time handling | [chrono](https://github.com/chronotope/chrono) |
| Error handling | [anyhow](https://github.com/dtolnay/anyhow) |

## Quick Start

```bash
# Clone
git clone https://github.com/Open330/open330-repo-pulse.git
cd open330-repo-pulse

# Run with defaults (scans open330)
cargo run --

# Scan another org and emit markdown report
cargo run -- --org rust-lang --max-repos 80 --stale-days 60 --format markdown --sort updated

# Include private repositories (requires token scope)
export GITHUB_TOKEN=ghp_your_token
cargo run -- --org Open330 --include-private --format table
```

### Run Tests

```bash
cargo test
```

### Typecheck & Lint

```bash
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

### Build

```bash
cargo build --release
```

## Deploy

No server deployment is required. For team distribution:

1. Build release binary: `cargo build --release`
2. Publish source to GitHub
3. Optionally install locally with `cargo install --path .`

Environment note: `GITHUB_TOKEN` is optional for public scans, and mandatory when `--include-private` is used.

## Acknowledgements

- **[GitHub REST API](https://docs.github.com/en/rest/repos/repos)** for repository metadata.
- **Rust CLI ecosystem** (`clap`, `reqwest`, `serde`, `chrono`, `anyhow`) for fast, reliable tooling.

## License

MIT

---

<p align="center">
  <sub>Built for maintainers who need a fast, scriptable repository health pulse.</sub>
</p>
