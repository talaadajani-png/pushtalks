#!/bin/sh
# Installs the freshly built PushTalks.app into /Applications and reopens it.
# Refuses while a meeting is being recorded (a recording file changed in the last 10 s),
# because replacing the app quits it.
set -e
APP="src-tauri/target/release/bundle/macos/PushTalks.app"
REC="$HOME/Library/Application Support/PushTalks/recordings"
if [ -d "$REC" ] && [ -n "$(find "$REC" -name '*.wav' -mtime -10s 2>/dev/null)" ]; then
  echo "install-local: a meeting is being recorded; not installing. Try again after it stops."
  exit 1
fi
if pgrep -f "/Applications/PushTalks.app/Contents/MacOS/pushtalks$" >/dev/null; then
  osascript -e 'tell application id "agency.push.pushtalks" to quit' || true
  i=0
  while pgrep -f "/Applications/PushTalks.app/Contents/MacOS/pushtalks$" >/dev/null && [ $i -lt 40 ]; do
    sleep 0.5; i=$((i + 1))
  done
fi
rm -rf /Applications/PushTalks.app
cp -R "$APP" /Applications/
open /Applications/PushTalks.app
echo "install-local: installed and opened PushTalks"
