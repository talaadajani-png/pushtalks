#!/bin/sh
# Publishes the current main branch to the private GitHub repo as one new snapshot commit.
# Local history is never pushed (it holds the archived prototype and early drafts); GitHub
# gets a chain of snapshots instead. Commits use the work email.
set -e
REMOTE="${1:-origin}"
git diff --quiet && git diff --cached --quiet || { echo "sync-github: commit your changes first"; exit 1; }
TREE=$(git rev-parse "main^{tree}")
git fetch -q "$REMOTE" main 2>/dev/null || true
PARENT=$(git rev-parse -q --verify "$REMOTE/main" || true)
if [ -n "$PARENT" ] && [ "$(git rev-parse "$PARENT^{tree}")" = "$TREE" ]; then
  echo "sync-github: GitHub is already up to date"
  exit 0
fi
MSG="Sync from main $(git rev-parse --short main): $(git log -1 --format=%s main)"
COMMIT=$(GIT_AUTHOR_EMAIL=taldajani@push.agency GIT_COMMITTER_EMAIL=taldajani@push.agency \
  git commit-tree "$TREE" ${PARENT:+-p "$PARENT"} -m "$MSG")
git push -q "$REMOTE" "$COMMIT:refs/heads/main"
echo "sync-github: pushed $COMMIT to $REMOTE/main"
