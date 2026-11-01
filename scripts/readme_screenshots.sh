#!/bin/sh
# Regenerates the README screenshots in docs/screenshots from a demo window
# (scripts/screenshot-demo.swift), so they never show a real desktop.
#
#   scripts/readme_screenshots.sh              builds dist/PixlSnapz.app first
#   SKIP_BUNDLE=1 scripts/readme_screenshots.sh
#
# Needs cliclick (brew install cliclick), Accessibility permission for the
# terminal (to move the pointer and press keys), and Screen Recording for
# dist/PixlSnapz.app. It runs its own PixlSnapz with separate settings and
# the shortcut ⌃⌥M, so a PixlSnapz you have installed is left alone (quit
# it if it uses ⌃⌥M). Keys are only sent once the overlay has focus.
set -eu
cd "$(dirname "$0")/.."

command -v cliclick >/dev/null || { echo "error: needs cliclick (brew install cliclick)" >&2; exit 1; }
if [ -z "${SKIP_BUNDLE:-}" ]; then
    scripts/bundle.sh
fi
APP=dist/PixlSnapz.app/Contents/MacOS/pixlsnapz
OUT=docs/screenshots
WORK=target/readme-screenshots
rm -rf "$WORK"
mkdir -p "$WORK/home" "$OUT"
DEMO_PID=""
APP_PID=""
cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
    [ -n "$DEMO_PID" ] && kill "$DEMO_PID" 2>/dev/null || true
}
trap cleanup EXIT

swiftc -O scripts/screenshot-demo.swift -o "$WORK/demo"
"$WORK/demo" > "$WORK/demo.txt" &
DEMO_PID=$!
for _ in $(seq 50); do grep -q ready "$WORK/demo.txt" 2>/dev/null && break; sleep 0.1; done

# rect NAME: "x y width height" of a demo element, in screen points.
rect() { awk -v n="$1" '$1 == n { print $2, $3, $4, $5 }' "$WORK/demo.txt"; }
# at NAME DX DY: a point inside an element, as "x,y" for cliclick.
at() { rect "$1" | awk -v dx="$2" -v dy="$3" '{ printf "%d,%d", $1 + dx, $2 + dy }'; }
set -- $(rect content)
REGION="$1,$2,$3,$4"

start_app() {
    PIXLSNAPZ_HOME="$PWD/$WORK/home" PIXLSNAPZ_LOG=pixlsnapz=debug "$@" "$APP" >> "$WORK/app.log" 2>&1 &
    APP_PID=$!
    sleep 2
}

cat > "$WORK/home/settings.toml" <<EOF
shortcut = "ctrl+opt+m"
show_hints = false
loupe = false
color = "#FF2D78"
EOF
start_app env

# measure: starts measuring with the shortcut and waits until the overlay
# has keyboard focus.
measure() {
    before=$(grep -c "overlay ready" "$WORK/app.log" || true)
    cliclick kd:ctrl,alt t:m ku:ctrl,alt
    started=""
    for _ in $(seq 30); do
        if [ "$(grep -c "overlay ready" "$WORK/app.log" || true)" -gt "$before" ]; then
            started=1
            break
        fi
        sleep 0.1
    done
    if [ -z "$started" ] || ! grep "overlay ready" "$WORK/app.log" | tail -1 | grep -q "key: true"; then
        echo "error: the overlay did not get keyboard focus; see $WORK/app.log" >&2
        exit 1
    fi
}
shot() { sleep 0.4; screencapture -x -R "$REGION" "$OUT/$1.png"; echo "Wrote $OUT/$1.png"; }
# finish: ends measuring with a right click on an empty spot (which works
# whatever has keyboard focus) and waits until it has ended.
finish() {
    before=$(grep -c "measuring ended" "$WORK/app.log" || true)
    cliclick rc:"$(at content 940 580)"
    for _ in $(seq 30); do
        [ "$(grep -c "measuring ended" "$WORK/app.log" || true)" -gt "$before" ] && return
        sleep 0.1
    done
    echo "error: measuring did not end; see $WORK/app.log" >&2
    exit 1
}

# 1. Distances from the pointer, inside a card's cover.
cliclick m:"$(at card2 70 50)"
measure
cliclick m:"$(at card2 71 51)"
shot distances
finish

# 2. Areas snapped to the button and the team member, and guides.
cliclick m:"$(at card1 30 30)"
measure
cliclick m:"$(at card1 0 60)" w:100 t:v w:100 m:"$(at card3 211 60)" w:100 t:v
cliclick dd:"$(at button -14 -12)" w:80 dm:"$(at button 70 20)" w:80 dm:"$(at button 156 52)" w:80 du:"$(at button 156 52)"
cliclick dd:"$(at avatar -10 -10)" w:80 dm:"$(at avatar 120 30)" w:80 dm:"$(at avatar 210 70)" w:80 du:"$(at avatar 210 70)"
cliclick m:"$(at card3 100 135)"
shot areas
finish

# 3. A line with its angle, a circle, and the loupe.
cliclick m:"$(at card1 20 20)"
measure
cliclick t:z w:100
cliclick kd:alt dd:"$(at card1 16 100)" w:80 dm:"$(at card2 100 60)" w:80 dm:"$(at card3 196 16)" w:80 du:"$(at card3 196 16)" ku:alt
cliclick t:e w:100 dd:"$(at avatar 28 28)" w:80 dm:"$(at avatar 40 28)" w:80 dm:"$(at avatar 56 28)" w:80 du:"$(at avatar 56 28)" t:e
# The loupe on a cover's corner, once the tool message is gone.
cliclick m:"$(at card2 17 99)" w:100 m:"$(at card2 16 99)"
sleep 1.5
shot tools
finish

# 4. Settings, Measuring tab.
kill "$APP_PID" 2>/dev/null || true
start_app env PIXLSNAPZ_SHOW_SETTINGS=measuring
sleep 1
set -- $(osascript -e 'tell application "System Events" to tell (first process whose unix id is '"$APP_PID"') to get {position, size} of window 1' | tr -d ',')
screencapture -x -R "$1,$2,$3,$4" "$OUT/settings.png"
echo "Wrote $OUT/settings.png"
