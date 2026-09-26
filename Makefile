SHELL := /bin/sh

.PHONY: build release check test install-desktop deb rpm pacman package appimage dmg msi clean

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

appimage:
	./scripts/package-appimage.sh

dmg:
	./scripts/package-dmg.sh

msi:
	powershell -File scripts/package-msi.ps1

clean:
	cargo clean
