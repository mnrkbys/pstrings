#!/usr/bin/env bash
# Prints the release notes for a tag: its CHANGELOG.md section, followed by a
# link comparing it with the previous release. The previous tag comes from the
# section's compare link at the end of the changelog.
#
# Usage: release-notes.sh <tag> <owner/repo> [changelog]
set -euo pipefail

tag=${1:?usage: release-notes.sh <tag> <owner/repo> [changelog]}
repo=${2:?usage: release-notes.sh <tag> <owner/repo> [changelog]}
changelog=${3:-CHANGELOG.md}
version=${tag#v}

awk -v version="$version" -v tag="$tag" -v repo="$repo" '
  function fail(message) { print "release-notes: " message > "/dev/stderr"; failed = 1; exit 1 }
  index($0, "## [" version "]") == 1 {
    if ($0 !~ /^## \[[^]]+\] - [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/)
      fail("the " version " heading has no release date: " $0)
    found = 1; inside = 1; next
  }
  inside && (/^## / || /^\[[^]]+\]: /) { inside = 0 }
  inside { lines[++n] = $0 }
  index($0, "[" version "]: ") == 1 {
    if ($0 !~ /\/compare\/[^\/]+\.\.\.[^\/]+$/)
      fail("the " version " link is not a compare link: " $0)
    previous = $0
    sub(/.*\/compare\//, "", previous)
    sub(/\.\.\..*/, "", previous)
  }
  END {
    if (failed) exit 1
    if (!found) fail("no section for " version)
    if (previous == "") fail("no compare link for " version)
    first = 1; while (first <= n && lines[first] == "") first++
    last = n; while (last >= first && lines[last] == "") last--
    for (i = first; i <= last; i++) print lines[i]
    print ""
    print "**Full Changelog**: https://github.com/" repo "/compare/" previous "..." tag
  }
' "$changelog"
