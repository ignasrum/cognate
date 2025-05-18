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
