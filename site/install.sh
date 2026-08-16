#!/bin/sh

set -eu

repository="bahadirarda/pkgshift"
requested_version="${PKGSHIFT_VERSION:-latest}"
install_dir="${PKGSHIFT_INSTALL_DIR:-${XDG_BIN_HOME:-}}"
temporary_dir=""

say() {
  printf '%s\n' "pkgshift: $*"
}

fail() {
  printf '%s\n' "pkgshift: error: $*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Install the pkgshift native CLI from a verified GitHub Release.

Usage:
  install.sh [--version <tag>] [--to <directory>]

Options:
  --version <tag>      Install an exact stable tag, such as v0.20260816.0.
  --to <directory>    Install pkgshift into this directory.
  -h, --help          Show this help message.

Environment:
  PKGSHIFT_VERSION       Exact release tag or version.
  PKGSHIFT_INSTALL_DIR   Destination directory.
  XDG_BIN_HOME           Fallback destination before $HOME/.local/bin.
EOF
}

cleanup() {
  if [ -n "$temporary_dir" ] && [ -d "$temporary_dir" ]; then
    rm -rf -- "$temporary_dir"
  fi
}

trap cleanup EXIT
trap 'exit 130' HUP INT TERM

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || fail "--version requires a value"
      requested_version="$2"
      shift 2
      ;;
    --to)
      [ "$#" -ge 2 ] || fail "--to requires a directory"
      install_dir="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown option: $1"
      ;;
  esac
done

for command_name in curl grep awk tar uname mktemp; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
done

case "$(uname -s)" in
  Linux)
    platform="unknown-linux-gnu"
    ;;
  Darwin)
    platform="apple-darwin"
    ;;
  *)
    fail "this installer supports Linux and macOS; use GitHub Releases for other platforms"
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64)
    architecture="x86_64"
    ;;
  arm64|aarch64)
    architecture="aarch64"
    ;;
  *)
    fail "unsupported architecture: $(uname -m)"
    ;;
esac

if [ "$requested_version" = "latest" ]; then
  latest_url="$(
    curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
      --retry 3 --output /dev/null --write-out '%{url_effective}' \
      "https://github.com/$repository/releases/latest"
  )"
  release_tag="${latest_url##*/}"
else
  case "$requested_version" in
    v*) release_tag="$requested_version" ;;
    *) release_tag="v$requested_version" ;;
  esac
fi

printf '%s\n' "$release_tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' \
  || fail "release version must match vX.Y.Z: $release_tag"

if [ -z "$install_dir" ]; then
  [ -n "${HOME:-}" ] || fail "HOME is not set; pass --to or PKGSHIFT_INSTALL_DIR"
  install_dir="$HOME/.local/bin"
fi

target="$architecture-$platform"
archive="pkgshift-$release_tag-$target.tar.gz"
download_root="https://github.com/$repository/releases/download/$release_tag"
temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/pkgshift-install.XXXXXX")"

say "downloading $release_tag for $target"
curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error --retry 3 \
  --output "$temporary_dir/$archive" "$download_root/$archive"
curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error --retry 3 \
  --output "$temporary_dir/SHA256SUMS" "$download_root/SHA256SUMS"

checksum="$(awk -v archive="$archive" '$2 == archive { print $1 }' "$temporary_dir/SHA256SUMS")"
[ "$(printf '%s' "$checksum" | wc -c | tr -d ' ')" = "64" ] \
  || fail "release checksum is missing or invalid for $archive"

say "verifying SHA-256 checksum"
if command -v sha256sum >/dev/null 2>&1; then
  (
    cd "$temporary_dir"
    printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --status
  ) || fail "checksum verification failed"
elif command -v shasum >/dev/null 2>&1; then
  (
    cd "$temporary_dir"
    printf '%s  %s\n' "$checksum" "$archive" | shasum -a 256 --check --status
  ) || fail "checksum verification failed"
else
  fail "sha256sum or shasum is required to verify the release"
fi

tar -xzf "$temporary_dir/$archive" -C "$temporary_dir"
source_binary="$temporary_dir/pkgshift-$release_tag-$target/pkgshift"
[ -f "$source_binary" ] || fail "release archive does not contain the pkgshift executable"

mkdir -p "$install_dir" || fail "cannot create install directory: $install_dir"
destination="$install_dir/pkgshift"
if command -v install >/dev/null 2>&1; then
  install -m 0755 "$source_binary" "$destination" \
    || fail "cannot install pkgshift to $destination"
else
  cp "$source_binary" "$destination" || fail "cannot copy pkgshift to $destination"
  chmod 0755 "$destination" || fail "cannot mark pkgshift as executable"
fi

"$destination" --version >/dev/null 2>&1 || fail "installed executable failed its version check"

say "installed $release_tag to $destination"
case ":${PATH:-}:" in
  *:"$install_dir":*) ;;
  *) say "add $install_dir to PATH before running pkgshift" ;;
esac
