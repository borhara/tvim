#!/usr/bin/env bash
# Cuts a release from your Mac: release-plz bumps the version and writes the changelog from
# the commits since the last tag, then this builds the binaries, commits, tags, publishes to
# crates.io, pushes to origin, makes a GitHub release with the binaries and install.sh, and
# points the Homebrew formula at them. Only feat, fix, perf and breaking commits make a
# release; chore, test, docs and the rest ride along with the next one.
#
#   scripts/release.sh
#   scripts/release.sh --dry-run    only build and package the binaries, into target/dist
#
# Run it on an up-to-date main once CI has passed on Tangled. It needs macOS (for the Apple
# targets), release-plz, zig and cargo-zigbuild (`brew install release-plz zig
# cargo-zigbuild`), the targets below added with rustup, `cargo login` and `gh auth login`.

set -euo pipefail

# the same as release_commits in release-plz.toml
release_commits='^(feat|fix|perf)(\(.*\))?!?:|^[a-z]+(\(.*\))?!:'
targets=(aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl aarch64-unknown-linux-musl)

dry_run=false
case "${1:-}" in
    "") ;;
    --dry-run) dry_run=true ;;
    *)
        echo "usage: scripts/release.sh [--dry-run]" >&2
        exit 2
        ;;
esac

version() {
    awk -F'"' '/^version = /{ print $2; exit }' Cargo.toml
}

cd "$(dirname "$0")/.."

if [[ "$(uname -s)" != Darwin ]]; then
    echo "the macOS binaries can only be built on a Mac" >&2
    exit 1
fi
tools=(zig cargo-zigbuild)
$dry_run || tools+=(release-plz gh)
for tool in "${tools[@]}"; do
    if ! command -v "$tool" >/dev/null; then
        echo "$tool isn't installed" >&2
        exit 1
    fi
done
installed="$(rustup target list --installed)"
for target in "${targets[@]}"; do
    if ! grep -qx "$target" <<<"$installed"; then
        echo "add the $target target first: rustup target add ${targets[*]}" >&2
        exit 1
    fi
done

# each target's binary, built and packed with the license and readme into dist
build() {
    local dist="$1"
    for target in "${targets[@]}"; do
        case "$target" in
            # zig links them, statically, so they run on any distro
            *-linux-musl) cargo zigbuild --release --locked --target "$target" ;;
            *) cargo build --release --locked --target "$target" ;;
        esac
        mkdir "$dist/$target"
        cp "target/$target/release/tvim" LICENSE README.md "$dist/$target/"
        tar czf "$dist/tvim-$target.tar.gz" -C "$dist/$target" tvim LICENSE README.md
    done
}

if $dry_run; then
    rm -rf target/dist
    mkdir -p target/dist
    build target/dist
    target/dist/aarch64-apple-darwin/tvim --version
    arch -x86_64 target/dist/x86_64-apple-darwin/tvim --version
    ls -lh target/dist/*.tar.gz
    exit 0
fi

if [[ "$(git branch --show-current)" != main ]]; then
    echo "releases are cut from main" >&2
    exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
    echo "commit or stash your changes first" >&2
    exit 1
fi
git fetch --quiet --tags origin
if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]]; then
    echo "main isn't the same as origin/main; pull or push first" >&2
    exit 1
fi

last="$(git describe --tags --abbrev=0 --match 'v*')"
# read in full first: grep -q stops at the first match, and pipefail would count git log's
# broken pipe as finding nothing
subjects="$(git log --format=%s "$last..HEAD")"
if ! grep -qE "$release_commits" <<<"$subjects"; then
    echo "nothing to release since $last"
    exit 0
fi

current="$(version)"
release-plz update
next="$(version)"
if [[ "$next" == "$current" ]]; then
    echo "release-plz found nothing to release since $last"
    exit 0
fi

git --no-pager diff CHANGELOG.md
read -rp "Release v$next? [y/N] " answer
if [[ "$answer" != [yY] ]]; then
    git checkout -- .
    echo "left as it was"
    exit 0
fi

# built before anything is published, so a target that fails to build stops the release
# with nothing to take back
dist="$(mktemp -d)"
trap 'rm -rf "$dist"' EXIT
build "$dist"

# this version's part of the changelog, for the tag's message and the GitHub release
notes="$(awk -v v="$next" '$0 ~ "^## \\[" v "\\]" { f = 1; next } /^## \[/ { if (f) exit } f' CHANGELOG.md |
    sed '/./,$!d')"

git commit --quiet -am "chore: release v$next"
# verbatim, or git would drop the changelog's ### headings as comments
git tag -a "v$next" --cleanup=verbatim -m "v$next" -m "$notes"
# publish before pushing, so a publish that fails leaves nothing to take back but the
# local commit and tag
cargo publish
git push --atomic origin HEAD:refs/heads/main "refs/tags/v$next"

# the GitHub mirror's releases page, which install.sh and the formula download from
gh release create "v$next" --repo borhara/tvim --title "v$next" --notes "$notes" \
    "$dist"/tvim-*.tar.gz install.sh

sha() {
    shasum -a 256 "$dist/tvim-$1.tar.gz" | cut -d' ' -f1
}
url="https://github.com/borhara/tvim/releases/download/v$next"
gh repo clone borhara/homebrew-tap "$dist/tap" -- --quiet
mkdir -p "$dist/tap/Formula"
cat >"$dist/tap/Formula/tvim.rb" <<EOF
class Tvim < Formula
  desc "Vim-like terminal text editor written in Rust"
  homepage "https://github.com/borhara/tvim"
  version "$next"
  license "MIT"

  on_macos do
    on_arm do
      url "$url/tvim-aarch64-apple-darwin.tar.gz"
      sha256 "$(sha aarch64-apple-darwin)"
    end
    on_intel do
      url "$url/tvim-x86_64-apple-darwin.tar.gz"
      sha256 "$(sha x86_64-apple-darwin)"
    end
  end

  on_linux do
    on_arm do
      url "$url/tvim-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$(sha aarch64-unknown-linux-musl)"
    end
    on_intel do
      url "$url/tvim-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$(sha x86_64-unknown-linux-musl)"
    end
  end

  def install
    bin.install "tvim"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tvim --version")
  end
end
EOF
git -C "$dist/tap" add Formula/tvim.rb
git -C "$dist/tap" commit --quiet -m "tvim $next"
git -C "$dist/tap" push --quiet
