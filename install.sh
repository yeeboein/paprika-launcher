#!/bin/sh
# Paprika Launcher installer for Linux (no root needed).
# Installs the app, its menu entry and the logo icon for your user.
#   ./install.sh            install
#   ./install.sh uninstall  remove (keeps your instances and worlds)
set -e
cd "$(dirname "$0")"

BIN="$HOME/.local/bin"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
SIZES="16 24 32 48 64 128 256 512"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t "$ICONS" 2>/dev/null || true
    command -v kbuildsycoca6 >/dev/null 2>&1 && kbuildsycoca6 >/dev/null 2>&1 || true
    command -v kbuildsycoca5 >/dev/null 2>&1 && kbuildsycoca5 >/dev/null 2>&1 || true
}

if [ "$1" = "uninstall" ]; then
    rm -f "$BIN/paprika" "$APPS/paprika.desktop"
    for s in $SIZES; do rm -f "$ICONS/${s}x${s}/apps/paprika.png"; done
    refresh
    echo "Paprika Launcher removed. Your instances are still in ~/.local/share/paprika"
    exit 0
fi

# Works from the release archive or from a source checkout after cargo build.
if [ -f ./paprika ]; then SRC=./paprika
elif [ -f target/release/paprika ]; then SRC=target/release/paprika
else echo "Can't find the paprika program. Run 'cargo build --release' first."; exit 1; fi
if [ -d assets/icons ]; then ICONDIR=assets/icons; else ICONDIR=icons; fi
if [ -f assets/paprika.desktop ]; then DESKTOP=assets/paprika.desktop; else DESKTOP=paprika.desktop; fi

mkdir -p "$APPS"
install -Dm755 "$SRC" "$BIN/paprika"
for s in $SIZES; do
    install -Dm644 "$ICONDIR/paprika-$s.png" "$ICONS/${s}x${s}/apps/paprika.png"
done
# Point the menu entry at the full path so it works even if ~/.local/bin isn't on PATH.
sed "s|^Exec=.*|Exec=$BIN/paprika|" "$DESKTOP" > "$APPS/paprika.desktop.tmp"
mv "$APPS/paprika.desktop.tmp" "$APPS/paprika.desktop"
chmod 644 "$APPS/paprika.desktop"
refresh
echo "Paprika Launcher installed! Find it in your app menu."
