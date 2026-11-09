# hyprview task runner — run `just` to list recipes.

# Show available recipes.
default:
	@just --list

# Pre-PR checklist: format, lint, test.
check: fmt-check clippy test

# Verify formatting without writing changes.
fmt-check:
	cargo fmt --all --check

# Apply formatting.
fmt:
	cargo fmt --all

# Lint all targets, treating warnings as errors.
clippy:
	cargo clippy --all-targets --locked -- -D warnings

# Run the unit test suite (no compositor required).
test:
	cargo test --locked

# Build the release binary.
build:
	cargo build --release --locked

# Preview the project site locally at http://localhost:8000.
site:
	rm -rf target/site-preview
	mkdir -p target/site-preview/assets
	cp site/index.html site/style.css target/site-preview/
	cp assets/logo.svg demo.gif target/site-preview/assets/
	python3 -m http.server -d target/site-preview 8000

# Install the release binary to ~/.cargo/bin.
install:
	cargo install --path . --locked --force

# Audit dependencies for known security advisories (needs cargo-deny).
audit:
	cargo deny check advisories

# One-time: point git at the tracked .githooks/ directory.
install-hooks:
	./install-hooks.sh

# Build the tracked Arch package in packaging/aur/hyprview-git.
package:
	cd packaging/aur/hyprview-git && makepkg -f

# Regenerate the AUR .SRCINFO after editing the PKGBUILD.
srcinfo:
	cd packaging/aur/hyprview-git && makepkg --printsrcinfo > .SRCINFO
