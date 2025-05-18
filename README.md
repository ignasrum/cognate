# cognate
Note taking app

Cognate is a simple note taking application built with the Iced GUI library for Rust. It allows you to create, manage, and visualize your notes stored in a local directory structure.

## Features

- **Markdown Editor:** Edit your notes using a built-in Markdown editor.
- **Note Organization:** Notes are stored in a directory structure within a designated notebook path.
- **Note Management:**
    - Create new notes (with support for nested directories).
    - Delete existing notes.
    - Move and rename notes and folders within the notebook.
- **Labeling:** Add and remove labels to individual notes for better organization.
- **Visualizer:** View your notes grouped by their labels.
- **Configuration:** Customize the application theme and notebook path via a `config.json` file.

## Getting Started

### Prerequisites

- Rust and Cargo installed (see [https://rustup.rs/](https://rustup.rs/) for installation instructions).

### Building and Running

You can build and run the application using the provided `Makefile`. Here are the relevant commands:

- `make build`: Builds the project in release mode.
- `make run`: Runs the application.
- `make install`: Builds and installs the application to `~/.local/bin`.
- `make clean`: Cleans the build artifacts.
- `make help`: Displays usage information for the make targets.

You can also specify `debug=1` to build in debug mode, for example: `make build debug=1`.

Alternatively, you can use Cargo commands directly:

```bash
cargo build --release # or cargo build for debug
cargo run --release # or cargo run
```

### Configuration

Cognate reads its configuration from a `config.json` file. By default, it looks for `./config.json` in the directory where the executable is run. You can specify a different path using the `COGNATE_CONFIG_PATH` environment variable.

A sample `config.json` looks like this:

```cognate/config.json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/{USER}/Documents/cognate/example_notebook"
}
