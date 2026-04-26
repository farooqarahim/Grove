#!/usr/bin/env bash
set -euo pipefail

# ── Ensure toolchain paths are available ────────────────────────────────────
export PATH="$HOME/.cargo/bin:$HOME/.rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# ── Grove Release Manager ────────────────────────────────────────────────────
# Interactive release tool for Grove Desktop
# Run: ./scripts/release.sh

CONF="crates/grove-gui/src-tauri/tauri.conf.json"
PKG="crates/grove-gui/package.json"
CARGO_WS="Cargo.toml"
CHANGELOG="CHANGELOG.md"
WORKFLOW="release-desktop.yml"
RELEASES_REPO="Grove-Tools/grove-loom"

# ── Colors ───────────────────────────────────────────────────────────────────
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
BLUE='\033[0;34m'
RESET='\033[0m'

hr()    { printf "${DIM}────────────────────────────────────────────────────${RESET}\n"; }
info()  { printf "${CYAN}▸${RESET} %s\n" "$1"; }
ok()    { printf "${GREEN}✓${RESET} %s\n" "$1"; }
warn()  { printf "${YELLOW}⚠${RESET} %s\n" "$1"; }
err()   { printf "${RED}✗${RESET} %s\n" "$1"; }

# ── Read current version ────────────────────────────────────────────────────
current_version() {
  grep '"version"' "$CONF" | head -1 | sed 's/.*"\([0-9]*\.[0-9]*\.[0-9]*\)".*/\1/'
}

bump_version() {
  local current="$1" type="$2"
  IFS='.' read -r major minor patch <<< "$current"
  case "$type" in
    patch) patch=$((patch + 1)) ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    major) major=$((major + 1)); minor=0; patch=0 ;;
  esac
  echo "${major}.${minor}.${patch}"
}

# ── Git helpers ──────────────────────────────────────────────────────────────
current_branch() { git rev-parse --abbrev-ref HEAD; }
has_uncommitted() { [ -n "$(git status --porcelain --untracked-files=no)" ]; }
tag_exists() { git rev-parse "$1" >/dev/null 2>&1; }
last_tag() { git describe --tags --abbrev=0 2>/dev/null || echo "none"; }

update_changelog() {
  local version="$1" notes="$2" date tmp
  date=$(date +%F)

  if grep -Fq "## [${version}]" "$CHANGELOG"; then
    warn "CHANGELOG already has an entry for ${version}; leaving it unchanged."
    return
  fi

  tmp=$(mktemp)
  awk -v version="$version" -v date="$date" -v notes="$notes" '
    function release_entry() {
      printf "## [%s] - %s\n\n", version, date
      printf "### Changed\n\n"
      printf "- %s\n\n", notes
    }

    /^## \[Unreleased\]/ {
      print
      in_unreleased = 1
      next
    }

    in_unreleased && /^## \[/ && !inserted {
      release_entry()
      inserted = 1
      in_unreleased = 0
    }

    { print }

    END {
      if (!inserted) {
        if (in_unreleased) {
          print ""
        }
        release_entry()
      }
    }
  ' "$CHANGELOG" > "$tmp"
  mv "$tmp" "$CHANGELOG"
}

# ── Header ───────────────────────────────────────────────────────────────────
show_header() {
  clear
  printf "\n${BOLD}  🌿 Grove Release Manager${RESET}\n"
  hr
  local cur
  cur=$(current_version)
  printf "  Version:  ${BOLD}%s${RESET}\n" "$cur"
  printf "  Branch:   ${BOLD}%s${RESET}\n" "$(current_branch)"
  printf "  Last tag: ${BOLD}%s${RESET}\n" "$(last_tag)"
  if has_uncommitted; then
    printf "  Tree:     ${YELLOW}dirty (uncommitted changes)${RESET}\n"
  else
    printf "  Tree:     ${GREEN}clean${RESET}\n"
  fi
  hr
}

# ── Menu: Main ───────────────────────────────────────────────────────────────
menu_main() {
  while true; do
    show_header
    printf "\n${BOLD}  What would you like to do?${RESET}\n\n"
    printf "  ${CYAN}1${RESET}  Release new version\n"
    printf "  ${CYAN}2${RESET}  View release history\n"
    printf "  ${CYAN}3${RESET}  Check workflow status\n"
    printf "  ${CYAN}4${RESET}  Re-run failed release\n"
    printf "  ${CYAN}5${RESET}  Delete a tag (undo release)\n"
    printf "  ${CYAN}6${RESET}  View changelog since last release\n"
    printf "  ${DIM}  q  Exit${RESET}\n"
    printf "\n  Choice: "
    read -r choice
    case "$choice" in
      1) menu_release ;;
      2) menu_history ;;
      3) menu_workflow_status ;;
      4) menu_rerun ;;
      5) menu_delete_tag ;;
      6) menu_changelog ;;
      q|Q) printf "\n"; exit 0 ;;
      *) ;;
    esac
  done
}

# ── Menu: Release ────────────────────────────────────────────────────────────
menu_release() {
  show_header

  # Preflight
  if has_uncommitted; then
    err "Working tree has uncommitted changes."
    printf "  Commit or stash your changes first.\n"
    printf "\n  Press enter to go back..."; read -r; return
  fi

  local cur
  cur=$(current_version)
  local v_patch v_minor v_major
  v_patch=$(bump_version "$cur" patch)
  v_minor=$(bump_version "$cur" minor)
  v_major=$(bump_version "$cur" major)

  printf "\n${BOLD}  Select version bump:${RESET}\n\n"
  printf "  ${CYAN}1${RESET}  Patch  ${DIM}${cur}${RESET} → ${GREEN}${v_patch}${RESET}  ${DIM}(bug fixes, small changes)${RESET}\n"
  printf "  ${CYAN}2${RESET}  Minor  ${DIM}${cur}${RESET} → ${GREEN}${v_minor}${RESET}  ${DIM}(new features, backward compatible)${RESET}\n"
  printf "  ${CYAN}3${RESET}  Major  ${DIM}${cur}${RESET} → ${GREEN}${v_major}${RESET}  ${DIM}(breaking changes)${RESET}\n"
  printf "  ${CYAN}4${RESET}  Custom version\n"
  printf "  ${DIM}  b  Back${RESET}\n"
  printf "\n  Choice: "
  read -r choice

  local new_version
  case "$choice" in
    1) new_version="$v_patch" ;;
    2) new_version="$v_minor" ;;
    3) new_version="$v_major" ;;
    4)
      printf "  Enter version (e.g. 1.0.0 or 1.0.0-beta.1): "
      read -r new_version
      if ! [[ "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.+-]+)?$ ]]; then
        err "Invalid version format. Use semver (e.g. 1.2.3 or 1.2.3-beta.1)"
        printf "\n  Press enter to go back..."; read -r; return
      fi
      ;;
    b|B) return ;;
    *) return ;;
  esac

  local tag="v${new_version}"

  if tag_exists "$tag"; then
    err "Tag $tag already exists."
    printf "\n  Press enter to go back..."; read -r; return
  fi

  # Release notes
  printf "\n  ${BOLD}Release notes${RESET} ${DIM}(optional, press enter to skip):${RESET}\n  > "
  read -r notes
  [ -z "$notes" ] && notes="Grove ${tag}"

  # Confirmation
  hr
  printf "\n${BOLD}  Release summary:${RESET}\n\n"
  printf "  Version:  ${cur} → ${GREEN}${BOLD}${new_version}${RESET}\n"
  printf "  Tag:      ${BOLD}${tag}${RESET}\n"
  printf "  Branch:   ${BOLD}$(current_branch)${RESET}\n"
  printf "  Notes:    ${notes}\n"
  printf "  Targets:  CLI + Desktop matrix (macOS arm64/x64, Linux x86_64, Windows x86_64)\n"
  printf "  Publish:  ${BLUE}github.com/${RELEASES_REPO}${RESET}\n"
  hr
  printf "\n  ${YELLOW}Proceed? (y/n):${RESET} "
  read -r confirm
  if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
    warn "Aborted."
    printf "\n  Press enter to go back..."; read -r; return
  fi

  # Execute release
  printf "\n"
  info "Bumping version to ${new_version}..."
  sed -i '' "s/\"version\": \"${cur}\"/\"version\": \"${new_version}\"/" "$CONF"
  sed -i '' "s/\"version\": \"${cur}\"/\"version\": \"${new_version}\"/" "$PKG"
  sed -i '' "s/^version = \"${cur}\"/version = \"${new_version}\"/" "$CARGO_WS"
  update_changelog "$new_version" "$notes"
  ok "Updated $CONF, $PKG, $CARGO_WS, $CHANGELOG"

  info "Regenerating Cargo.lock..."
  cargo check --quiet 2>/dev/null || cargo generate-lockfile --quiet
  ok "Cargo.lock updated"

  info "Committing..."
  git add "$CONF" "$PKG" "$CARGO_WS" "$CHANGELOG" Cargo.lock
  git commit -q -m "release: ${tag}"
  ok "Committed release: ${tag}"

  info "Creating signed tag ${tag}..."
  git tag -s "$tag" -m "$notes"
  ok "Signed tag created"

  info "Pushing to origin..."
  git push -q origin HEAD "$tag"
  ok "Pushed commit + tag"

  printf "\n${GREEN}${BOLD}  ✓ Released ${tag}${RESET}\n"
  printf "  Two workflows triggered:\n"
  printf "    ${DIM}release.yml${RESET}          → CLI archives on this repository\n"
  printf "    ${DIM}release-desktop.yml${RESET}  → Desktop app on ${RELEASES_REPO}\n"
  printf "  Run ${DIM}gh run list --workflow=${WORKFLOW} --limit 1${RESET} to check status.\n"
  printf "\n  Press enter to continue..."; read -r
}

# ── Menu: History ────────────────────────────────────────────────────────────
menu_history() {
  show_header
  printf "\n${BOLD}  Release history (last 15 tags):${RESET}\n\n"
  git tag -l 'v*' --sort=-version:refname | head -15 | while read -r tag; do
    local date
    date=$(git log -1 --format="%ci" "$tag" 2>/dev/null | cut -d' ' -f1)
    local msg
    msg=$(git tag -l --format='%(contents:subject)' "$tag")
    printf "  ${GREEN}%-12s${RESET} ${DIM}%s${RESET}  %s\n" "$tag" "$date" "$msg"
  done
  if [ -z "$(git tag -l 'v*')" ]; then
    printf "  ${DIM}No releases yet.${RESET}\n"
  fi
  printf "\n  Press enter to go back..."; read -r
}

# ── Menu: Workflow Status ────────────────────────────────────────────────────
menu_workflow_status() {
  show_header
  printf "\n${BOLD}  Recent workflow runs:${RESET}\n\n"
  if ! command -v gh &>/dev/null; then
    err "gh CLI not installed. Install: brew install gh"
    printf "\n  Press enter to go back..."; read -r; return
  fi
  gh run list --workflow="$WORKFLOW" --limit 5 2>&1 | while IFS=$'\t' read -r status _ title workflow ref event id elapsed created; do
    local color="$DIM"
    case "$status" in
      completed)   color="$GREEN" ;;
      in_progress) color="$YELLOW" ;;
      failure)     color="$RED" ;;
    esac
    printf "  ${color}%-12s${RESET} %-10s ${DIM}%s  %s${RESET}\n" "$status" "$ref" "$elapsed" "$id"
  done
  printf "\n  ${DIM}Open in browser: gh run view --web${RESET}\n"
  printf "\n  Press enter to go back..."; read -r
}

# ── Menu: Re-run ─────────────────────────────────────────────────────────────
menu_rerun() {
  show_header
  if ! command -v gh &>/dev/null; then
    err "gh CLI not installed."
    printf "\n  Press enter to go back..."; read -r; return
  fi

  printf "\n${BOLD}  Failed workflow runs:${RESET}\n\n"
  local runs
  runs=$(gh run list --workflow="$WORKFLOW" --status=failure --limit 5 --json databaseId,headBranch,createdAt \
    --jq '.[] | "\(.databaseId)\t\(.headBranch)\t\(.createdAt)"' 2>/dev/null || echo "")

  if [ -z "$runs" ]; then
    printf "  ${DIM}No failed runs.${RESET}\n"
    printf "\n  Press enter to go back..."; read -r; return
  fi

  local i=1
  while IFS=$'\t' read -r id branch created; do
    printf "  ${CYAN}%d${RESET}  Run #%s  ${DIM}%s  %s${RESET}\n" "$i" "$id" "$branch" "$created"
    i=$((i + 1))
  done <<< "$runs"

  printf "\n  Select run to re-trigger (or b to go back): "
  read -r pick
  [[ "$pick" == "b" || "$pick" == "B" ]] && return

  local run_id
  run_id=$(echo "$runs" | sed -n "${pick}p" | cut -f1)
  if [ -n "$run_id" ]; then
    info "Re-running workflow #${run_id}..."
    gh run rerun "$run_id" --failed
    ok "Re-triggered. Check with: gh run view $run_id"
  else
    err "Invalid selection."
  fi
  printf "\n  Press enter to go back..."; read -r
}

# ── Menu: Delete Tag ─────────────────────────────────────────────────────────
menu_delete_tag() {
  show_header
  printf "\n${BOLD}  Recent tags:${RESET}\n\n"
  local tags
  tags=$(git tag -l 'v*' --sort=-version:refname | head -10)
  if [ -z "$tags" ]; then
    printf "  ${DIM}No tags found.${RESET}\n"
    printf "\n  Press enter to go back..."; read -r; return
  fi

  local i=1
  while read -r tag; do
    printf "  ${CYAN}%d${RESET}  %s\n" "$i" "$tag"
    i=$((i + 1))
  done <<< "$tags"

  printf "\n  Select tag to delete (or b to go back): "
  read -r pick
  [[ "$pick" == "b" || "$pick" == "B" ]] && return

  local target
  target=$(echo "$tags" | sed -n "${pick}p")
  if [ -z "$target" ]; then
    err "Invalid selection."
    printf "\n  Press enter to go back..."; read -r; return
  fi

  printf "  ${RED}Delete tag ${BOLD}${target}${RESET}${RED} locally and from origin? (y/n):${RESET} "
  read -r confirm
  if [[ "$confirm" == "y" || "$confirm" == "Y" ]]; then
    git tag -d "$target" 2>/dev/null
    git push origin ":refs/tags/$target" 2>/dev/null || true
    ok "Deleted tag $target"
    warn "Note: if a release was published to ${RELEASES_REPO}, delete it manually on GitHub."
  else
    warn "Aborted."
  fi
  printf "\n  Press enter to go back..."; read -r
}

# ── Menu: Changelog ──────────────────────────────────────────────────────────
menu_changelog() {
  show_header
  local last
  last=$(last_tag)
  if [ "$last" = "none" ]; then
    printf "\n  ${DIM}No previous tags. Showing last 20 commits:${RESET}\n\n"
    git log --oneline -20 | sed 's/^/  /'
  else
    printf "\n${BOLD}  Changes since ${last}:${RESET}\n\n"
    git log --oneline "${last}..HEAD" | sed 's/^/  /'
    local count
    count=$(git rev-list --count "${last}..HEAD")
    printf "\n  ${DIM}${count} commits since ${last}${RESET}\n"
  fi
  printf "\n  Press enter to go back..."; read -r
}

# ── Entry point ──────────────────────────────────────────────────────────────
menu_main
