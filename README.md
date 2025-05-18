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

You can build and run the application using the provided `Makefile`:

```cognate/Makefile#L1-16
prog :=cognate

debug ?=

$(info debug is $(debug))

ifdef debug
  release :=
  target :=debug
else
  release :=--release
  target :=release
endif

build:
	cargo build $(release)

run:
	cargo run

clean:
	cargo clean

install:
	mkdir -p ~/bin
	cp target/$(target)/$(prog) ~/.local/bin/$(prog)

all: build install

help:
	@echo "usage: make $(prog) [debug=1]"
```

### Configuration

Cognate reads its configuration from a `config.json` file. By default, it looks for `./config.json` in the directory where the executable is run. You can specify a different path using the `COGNATE_CONFIG_PATH` environment variable.

A sample `config.json` looks like this:

```cognate/config.json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/ignasr/Documents/cognate/example_notebook"
}
