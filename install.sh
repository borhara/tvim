#!/bin/sh
# Installs the latest tvim release from GitHub.
#
#   curl -fsSL https://github.com/borhara/tvim/releases/latest/download/install.sh | sh
#
# Set TVIM_INSTALL_DIR to install somewhere other than ~/.local/bin.
set -eu

repo="borhara/tvim"
dir="${TVIM_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
    Darwin) os="apple-darwin" ;;
    Linux) os="unknown-linux-musl" ;;
    *) echo "tvim: no prebuilt binary for $(uname -s), use 'cargo install tvim'" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    arm64 | aarch64) arch="aarch64" ;;
    x86_64 | amd64) arch="x86_64" ;;
    *) echo "tvim: no prebuilt binary for $(uname -m), use 'cargo install tvim'" >&2; exit 1 ;;
esac

url="https://github.com/$repo/releases/latest/download/tvim-$arch-$os.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $url"
curl -fsSL "$url" | tar xz -C "$tmp"
mkdir -p "$dir"
mv "$tmp/tvim" "$dir/tvim"
chmod +x "$dir/tvim"
echo "installed $("$dir/tvim" --version) to $dir/tvim"

case ":$PATH:" in
    *":$dir:"*) ;;
    *)
        echo
        echo "$dir isn't on your PATH yet. To add it, run:"
        echo
        case "$(basename "${SHELL:-sh}")" in
            zsh) echo "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.zshrc" ;;
            bash) echo "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.bashrc" ;;
            fish) echo "  fish_add_path $dir" ;;
            *) echo "  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile" ;;
        esac
        echo
        echo "then open a new terminal."
        ;;
esac
