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

api:
	cargo run -p cognate-api

clean:
	cargo clean

install:
	cp target/$(target)/$(prog) ~/.local/bin/$(prog)

test:
	cargo test

format:
	cargo fmt --all
	cargo clippy --all-targets --fix --allow-dirty --allow-staged
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --all -- --check

all: build install

help:
	@echo "usage: make $(prog) [debug=1]"
	@echo "       make api"
