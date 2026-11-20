#!/bin/sh
# Paidva installer:
#   curl -fsSL https://raw.githubusercontent.com/dev-the-dev-while-deving/paidva-research-agent/main/install.sh | sh
# Downloads the latest macOS binary from GitHub Releases, checks its SHA-256, and installs it to ~/.local/bin
# (override with PAIDVA_BIN_DIR). Falls back to building from source if cargo is installed.
set -eu

REPO="dev-the-dev-while-deving/paidva-research-agent"
BIN_DIR="${PAIDVA_BIN_DIR:-$HOME/.local/bin}"

say() { printf '  %s\n' "$*"; }
fail() { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "Paidva needs macOS for now: its Python sandbox uses macOS sandbox-exec."
case "$(uname -m)" in
  arm64 | aarch64) target=aarch64-apple-darwin ;;
  x86_64) target=x86_64-apple-darwin ;;
  *) fail "unsupported CPU: $(uname -m)" ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
url="https://github.com/$REPO/releases/latest/download/paidva-$target.tar.gz"

printf '\n  \033[38;5;215m◆ Paidva\033[0m  installing for %s\n\n' "$target"
if curl -fsSL "$url" -o "$tmp/paidva.tar.gz" && curl -fsSL "$url.sha256" -o "$tmp/paidva.sha256"; then
  expected=$(cut -d' ' -f1 "$tmp/paidva.sha256")
  actual=$(shasum -a 256 "$tmp/paidva.tar.gz" | cut -d' ' -f1)
  [ "$expected" = "$actual" ] || fail "checksum mismatch: the download is corrupt or was tampered with"
  tar xzf "$tmp/paidva.tar.gz" -C "$tmp"
  mkdir -p "$BIN_DIR"
  install -m 755 "$tmp/paidva" "$BIN_DIR/paidva"
  say "✓ installed $BIN_DIR/paidva (checksum verified)"
elif command -v cargo >/dev/null 2>&1; then
  say "No prebuilt binary found; building from source with cargo (a few minutes)…"
  cargo install --locked --git "https://github.com/$REPO" --root "$tmp/root"
  mkdir -p "$BIN_DIR"
  install -m 755 "$tmp/root/bin/paidva" "$BIN_DIR/paidva"
  say "✓ built and installed $BIN_DIR/paidva"
else
  fail "couldn't download a release, and cargo isn't installed to build one (https://rustup.rs)"
fi

if ! command -v uv >/dev/null 2>&1; then
  say "! uv is needed for the Python sandbox. Install it with:"
  say "    curl -LsSf https://astral.sh/uv/install.sh | sh"
fi

if ! command -v ollama >/dev/null 2>&1; then
  say "Tip: run the model on your Mac (private, no keys, no limits). The setup can do this for you, or:"
  say "    brew install ollama && ollama pull qwen3.5:9b-q4_K_M"
fi

case ":$PATH:" in
  *":$BIN_DIR:"*) say "Run: paidva" ;;
  *)
    say "Add Paidva to your PATH, then run it:"
    say "    echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc"
    say "    paidva"
    ;;
esac
printf '\n'
