prog :=cognate

PROFILE ?= release

ifdef debug
  PROFILE := debug
endif

ifeq ($(PROFILE),release)
  cargo_profile := --release
  target :=release
else ifeq ($(PROFILE),debug)
  cargo_profile :=
  target :=debug
else
  $(error PROFILE must be either release or debug)
endif

$(info profile is $(PROFILE))

build:
	cargo build $(cargo_profile)

run:
	cargo run $(cargo_profile)

api:
	cargo run -p cognate-api $(cargo_profile)

clean:
	cargo clean

install:
	cp target/$(target)/$(prog) ~/.local/bin/$(prog)

test:
	cargo test

format:
	cargo fmt --all
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo fmt --all -- --check

all: build install

help:
	@echo "usage: make [PROFILE=release|debug] [target]"
	@echo "       make run"
	@echo "       make PROFILE=debug run"
	@echo "       make debug=1 run  (legacy alias)"
	@echo "       make api"
