# hyprview task runner — run `just` to list recipes.

# Note: formatting is intentionally not part of `check`. The source uses manual
# column alignment and `cargo fmt` would rewrite most of the tree.

# Show available recipes.
default:
	@just --list

# Pre-PR checklist: lint + test.
check: clippy test

# Lint all targets, treating warnings as errors.
clippy:
	cargo clippy --all-targets --locked -- -D warnings

# Run the unit test suite (no compositor required).
test:
	cargo test --locked

# Build the release binary.
build:
	cargo build --release --locked

# Install the release binary to ~/.cargo/bin.
install:
	cargo install --path . --locked --force

# Build the tracked Arch package in packaging/aur/hyprview-git.
package:
	cd packaging/aur/hyprview-git && makepkg -f

# Regenerate the AUR .SRCINFO after editing the PKGBUILD.
srcinfo:
	cd packaging/aur/hyprview-git && makepkg --printsrcinfo > .SRCINFO
