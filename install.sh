#!/bin/sh
# Paidva installer:
#   curl -fsSL https://raw.githubusercontent.com/dev-the-dev-while-deving/paidva-research-agent/main/install.sh | sh
# Downloads the latest binary for this machine (macOS, Linux, or Windows through WSL) from GitHub Releases,
# checks its SHA-256, and installs it to ~/.local/bin (override with PAIDVA_BIN_DIR).
set -eu

REPO="dev-the-dev-while-deving/paidva-research-agent"
BIN_DIR="${PAIDVA_BIN_DIR:-$HOME/.local/bin}"

say() { printf '  %s\n' "$*"; }
fail() { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

os=$(uname -s)
case "$os" in
  Darwin) sys=apple-darwin ;;
  Linux) sys=unknown-linux-gnu ;;
  MINGW* | MSYS* | CYGWIN*) fail "on Windows, install WSL (wsl --install in PowerShell), then run this command inside Ubuntu." ;;
  *) fail "unsupported system: $os (Paidva runs on macOS, Linux, and Windows through WSL)" ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) target="aarch64-$sys" ;;
  x86_64 | amd64) target="x86_64-$sys" ;;
  *) fail "unsupported CPU: $(uname -m)" ;;
esac
wsl=false
[ "$os" = Linux ] && grep -qi microsoft /proc/version 2>/dev/null && wsl=true

sha256() { if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"; else sha256sum "$1"; fi | cut -d' ' -f1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
url="https://github.com/$REPO/releases/latest/download/paidva-$target.tar.gz"

printf '\n  \033[38;5;215m◆ Paidva\033[0m  installing for %s%s\n\n' "$target" "$([ "$wsl" = true ] && echo ' (WSL)')"
curl -fsSL "$url" -o "$tmp/paidva.tar.gz" && curl -fsSL "$url.sha256" -o "$tmp/paidva.sha256" \
  || fail "no release for $target yet: see https://github.com/$REPO/releases"
expected=$(cut -d' ' -f1 "$tmp/paidva.sha256")
[ "$expected" = "$(sha256 "$tmp/paidva.tar.gz")" ] || fail "checksum mismatch: the download is corrupt or was tampered with"
tar xzf "$tmp/paidva.tar.gz" -C "$tmp"
mkdir -p "$BIN_DIR"
install -m 755 "$tmp/paidva" "$BIN_DIR/paidva"
say "✓ installed $BIN_DIR/paidva (checksum verified)"

if [ "$os" = Linux ] && ! command -v bwrap >/dev/null 2>&1; then
  say "! The Python sandbox needs bubblewrap. Install it with your package manager, e.g.:"
  say "    sudo apt install bubblewrap     (Debian, Ubuntu, WSL)"
  say "    sudo dnf install bubblewrap     (Fedora)"
fi

if ! command -v uv >/dev/null 2>&1; then
  if [ -t 0 ] && [ -r /dev/tty ]; then
    printf '  ! uv is needed for the Python sandbox. Install uv now? [Y/n] '
    read -r ans < /dev/tty || ans="n"
    case "$ans" in
      "" | y | Y | yes | YES)
        curl -LsSf https://astral.sh/uv/install.sh | sh
        ;;
      *)
        say "Install uv later with: curl -LsSf https://astral.sh/uv/install.sh | sh"
        ;;
    esac
  else
    say "! uv is needed for the Python sandbox. Install it with:"
    say "    curl -LsSf https://astral.sh/uv/install.sh | sh"
  fi
fi

if ! command -v ollama >/dev/null 2>&1; then
  say "Tip: run the model on this machine (private, no keys, no limits):"
  if [ "$os" = Darwin ]; then say "    brew install ollama && ollama pull qwen3.5:9b-q4_K_M"
  else say "    curl -fsSL https://ollama.com/install.sh | sh && ollama pull qwen3.5:9b-q4_K_M"; fi
fi

rc=~/.zshrc
[ "$os" = Linux ] && rc=~/.bashrc
case ":$PATH:" in
  *":$BIN_DIR:"*) say "Run: paidva" ;;
  *)
    say "Add Paidva to your PATH, then run it:"
    say "    echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> $rc && . $rc"
    say "    paidva"
    ;;
esac
printf '\n'
