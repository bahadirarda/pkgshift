#!/bin/sh

set -eu

repository="bahadirarda/pkgshift"
requested_version="${PKGSHIFT_VERSION:-latest}"
install_dir="${PKGSHIFT_INSTALL_DIR:-${XDG_BIN_HOME:-}}"
temporary_dir=""
skill_temporary=""

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
  PKGSHIFT_DATA_DIR      Shared data root for the bundled Agent Skill.
  XDG_BIN_HOME           Fallback destination before $HOME/.local/bin.
  XDG_DATA_HOME          Fallback shared data root before $HOME/.local/share.
EOF
}

cleanup() {
  if [ -n "$temporary_dir" ] && [ -d "$temporary_dir" ]; then
    rm -rf -- "$temporary_dir"
  fi
  if [ -n "$skill_temporary" ] && [ -d "$skill_temporary" ]; then
    rm -rf -- "$skill_temporary"
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

for command_name in curl grep awk tar uname mktemp cp mv; do
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
source_skill="$temporary_dir/pkgshift-$release_tag-$target/skills/pkgshift"
[ -f "$source_binary" ] || fail "release archive does not contain the pkgshift executable"
[ -f "$source_skill/SKILL.md" ] || fail "release archive does not contain the portable Agent Skill"

if [ -n "${PKGSHIFT_DATA_DIR:-}" ]; then
  data_dir="$PKGSHIFT_DATA_DIR"
elif [ -n "${XDG_DATA_HOME:-}" ]; then
  data_dir="$XDG_DATA_HOME/pkgshift"
else
  [ -n "${HOME:-}" ] || fail "HOME is not set; pass PKGSHIFT_DATA_DIR"
  data_dir="$HOME/.local/share/pkgshift"
fi

skill_parent="$data_dir/skills"
skill_destination="$skill_parent/pkgshift"
mkdir -p "$skill_parent" || fail "cannot create shared data directory: $skill_parent"
[ ! -L "$skill_destination" ] || fail "portable Agent Skill destination must not be a symbolic link"
if [ -e "$skill_destination" ] && [ ! -d "$skill_destination" ]; then
  fail "portable Agent Skill destination is not a directory: $skill_destination"
fi
skill_temporary="$skill_parent/.pkgshift.$$.tmp"
skill_backup="$skill_parent/.pkgshift.$$.backup"
[ ! -e "$skill_temporary" ] || fail "temporary Agent Skill path already exists"
[ ! -e "$skill_backup" ] || fail "backup Agent Skill path already exists"
cp -R "$source_skill" "$skill_temporary" \
  || fail "cannot stage the portable Agent Skill"
if [ -d "$skill_destination" ]; then
  mv "$skill_destination" "$skill_backup" \
    || fail "cannot prepare the existing portable Agent Skill for replacement"
  if ! mv "$skill_temporary" "$skill_destination"; then
    mv "$skill_backup" "$skill_destination" \
      || fail "cannot restore the previous portable Agent Skill from $skill_backup"
    fail "cannot install the portable Agent Skill"
  fi
  skill_temporary=""
  rm -rf -- "$skill_backup"
else
  mv "$skill_temporary" "$skill_destination" \
    || fail "cannot install the portable Agent Skill"
  skill_temporary=""
fi

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
PKGSHIFT_DATA_DIR="$data_dir" "$destination" skill status \
  --scope project --client codex --cwd "$temporary_dir" --json --non-interactive \
  >/dev/null 2>&1 || fail "installed executable could not resolve its portable Agent Skill"

say "installed $release_tag to $destination"
say "installed portable Agent Skill data to $skill_destination"
case ":${PATH:-}:" in
  *:"$install_dir":*) ;;
  *) say "add $install_dir to PATH before running pkgshift" ;;
esac
