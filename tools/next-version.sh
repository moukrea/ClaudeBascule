#!/usr/bin/env bash
# The next release from the conventional commits merged since the last tag.
#   tools/next-version.sh <notes file>   prints the version (without "v"), or nothing when there is nothing to release.
# Rules: a breaking change ("type!:" or "BREAKING CHANGE") bumps the major, "feat" the minor,
# anything else the patch. The first release is 0.1.0.
set -euo pipefail
notes=${1:-/dev/null}
last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
range=${last:+$last..}HEAD
mapfile -t subjects < <(git log --no-merges --format='%s' "$range")
[ ${#subjects[@]} -gt 0 ] || exit 0
bodies=$(git log --no-merges --format='%b' "$range")
level=patch
for s in "${subjects[@]}"; do
  if [[ $s =~ ^[a-z]+(\([^\)]*\))?!: ]]; then level=major; break; fi
  if [[ $s =~ ^feat(\([^\)]*\))?: ]]; then level=minor; fi
done
grep -q '^BREAKING[ -]CHANGE' <<<"$bodies" && level=major
if [ -z "$last" ]; then
  next=0.1.0
else
  IFS=. read -r ma mi pa <<<"${last#v}"
  case $level in
    major) next="$((ma + 1)).0.0" ;;
    minor) next="$ma.$((mi + 1)).0" ;;
    patch) next="$ma.$mi.$((pa + 1))" ;;
  esac
fi
{
  section() { # title, regex
    local out=""
    for s in "${subjects[@]}"; do
      if [[ $s =~ $2 ]]; then out+="- ${s#*: }"$'\n'; fi
    done
    [ -n "$out" ] && printf '## %s\n%s\n' "$1" "$out"
    return 0
  }
  section "Breaking changes" '^[a-z]+(\([^)]*\))?!:'
  section "New" '^feat(\([^)]*\))?:'
  section "Fixes" '^fix(\([^)]*\))?:'
  section "Faster" '^perf(\([^)]*\))?:'
  section "Other changes" '^(refactor|docs|build|ci|chore|test|style|revert)(\([^)]*\))?:'
  printf 'Install or update: see the [README](https://github.com/moukrea/ClaudeBascule#install).\n'
} > "$notes"
echo "$next"
