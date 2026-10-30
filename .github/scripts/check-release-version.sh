#!/usr/bin/env bash
# The release tag and the version the binary reports must agree (ruagent-issues-56-59 t43).
#
# v0.1.1 published archives whose `ruagent --version` said `ruagent 0.1.0`, because the tag was cut
# from a commit whose workspace version had never been bumped. Nothing failed: the gate checked that
# CI was green, the smoke test checked that the archive RUNS, and both were true. The mismatch is
# invisible until a user asks support 'which version do I have?'. So compare them here, before anything
# is built.
#
#   check-release-version.sh v1.2.3 [path/to/Cargo.toml]
set -euo pipefail

tag="${1:-}"
manifest="${2:-Cargo.toml}"

if [ -z "$tag" ]; then
  echo "usage: $0 <tag, e.g. v1.2.3> [Cargo.toml]" >&2
  exit 2
fi
if [ ! -f "$manifest" ]; then
  echo "::error::no workspace manifest at '$manifest'" >&2
  exit 1
fi

# `version` inside [workspace.package] -- the single authority every crate inherits via version.workspace.
version="$(awk '/^\[workspace.package\]/{w=1;next} /^\[/{w=0} w&&/^version[[:space:]]*=/{gsub(/["[:space:]]/,"",$0); sub(/^version=/,""); print; exit}' "$manifest")"

if [ -z "$version" ]; then
  echo "::error::could not read [workspace.package] version from $manifest -- the guard is blind, which is a failure, not a pass" >&2
  exit 1
fi

echo "tag=${tag}  workspace version=${version}"

if [ "$tag" != "v${version}" ]; then
  echo "::error::tag '${tag}' does not match the workspace version '${version}' (expected 'v${version}'). Bump [workspace.package] version in $manifest and commit it BEFORE tagging: the tag is what users download, the version is what the binary reports, and a release where those disagree cannot be supported." >&2
  exit 1
fi

echo "ok: ${tag} matches the workspace version"