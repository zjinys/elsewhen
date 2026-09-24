SHELL := /bin/sh

.PHONY: build release check test install-desktop deb rpm pacman package clean

build:
	cargo build

release:
	cargo build --release

check:
	cargo check

test:
	cargo test --all-targets

install-desktop:
	./scripts/install-linux-desktop.sh

deb:
	./scripts/package-deb.sh

rpm:
	./scripts/package-rpm.sh

pacman:
	./scripts/package-pacman.sh

package: deb rpm pacman

clean:
	cargo clean
