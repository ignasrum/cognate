# cognate

Cognate is a local-first note taking desktop app built with Rust and Iced. Notes are stored as folders on disk, edited as Markdown, and visualized through label-based relationships.

## Features

- Markdown editor with live preview
- Notebook-style file organization
- Note and folder create/delete/move flows
- Labeling for note categorization
- Embedded image workflow for pasted images
- Visualizer for label-connected notes
- Theme and UI scale configuration via `config.json`

## Quick Start

### Prerequisites

- Rust and Cargo (install from [https://rustup.rs/](https://rustup.rs/))

### Build and run

Use `make`:

- `make build` builds in release mode
- `make run` runs the app
- `make test` runs the full test suite
- `make help` lists all targets

Or use Cargo directly:

```bash
cargo build --release
cargo run --release
```

### Configuration

Cognate reads configuration from `./config.json` by default. Override with:

```bash
COGNATE_CONFIG_PATH=/path/to/config.json cargo run --release
```

Example:

```json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/{USER}/Documents/cognate/example_notebook",
  "scale": 1.0
}
```

- `theme` is the UI theme name
- `notebook_path` points to your notes root directory
- `scale` is the global UI scale and must be positive

## Documentation

- [Development guide](docs/DEVELOPMENT.md)
- [Architecture guide](docs/ARCHITECTURE.md)
- [Manual testing checklist](docs/MANUAL_TESTING.md)

## Project Layout

- `src/components` contains UI/editor components
- `src/notebook` implements note metadata, storage, operations, and search
- `src/configuration` handles config parsing and theme mapping
- `src/tests` contains integration-style unit tests across modules
