#!/usr/bin/env bash
# Install Spider Tauri for the current user (Manjaro / Arch / most Linux desktops).
# Does not need root: ~/.local/opt + ~/.local/bin + XDG applications menu.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="spider-tauri"
DISPLAY_NAME="Spider Tauri"
INSTALL_DIR="${SPIDER_INSTALL_DIR:-$HOME/.local/opt/$APP_NAME}"
BIN_DIR="${SPIDER_BIN_DIR:-$HOME/.local/bin}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_DIR="$DATA_HOME/applications"
PIXMAP_DIR="$DATA_HOME/pixmaps"
ICON_THEME_ROOT="$DATA_HOME/icons/hicolor"
BINARY=""

# Collect candidates and pick the newest by mtime (stale dist/ must not win over a fresh build).
candidates=()
for candidate in \
    "$ROOT/dist/$APP_NAME" \
    "$ROOT/src-tauri/target/release/$APP_NAME" \
    "${CARGO_TARGET_DIR:-}/release/$APP_NAME"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        candidates+=("$candidate")
    fi
done

while IFS= read -r -d '' path; do
    if [[ -x "$path" && "$(basename "$path")" == "$APP_NAME" ]]; then
        candidates+=("$path")
    fi
done < <(find "$ROOT/src-tauri/target/release/bundle" -type f -name "$APP_NAME" -print0 2>/dev/null || true)

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    while IFS= read -r -d '' path; do
        if [[ -x "$path" && "$(basename "$path")" == "$APP_NAME" ]]; then
            candidates+=("$path")
        fi
    done < <(find "$CARGO_TARGET_DIR/release/bundle" -type f -name "$APP_NAME" -print0 2>/dev/null || true)
fi

newest_mtime=0
for candidate in "${candidates[@]+"${candidates[@]}"}"; do
    mtime=$(stat -c '%Y' "$candidate" 2>/dev/null || echo 0)
    if (( mtime >= newest_mtime )); then
        newest_mtime=$mtime
        BINARY="$candidate"
    fi
done

if [[ -z "$BINARY" ]]; then
    echo "Не знайдено бінарник $APP_NAME." >&2
    echo "Спочатку: npm run build" >&2
    echo "Очікується один з шляхів:" >&2
    echo "  dist/$APP_NAME" >&2
    echo "  src-tauri/target/release/$APP_NAME" >&2
    echo "  \$CARGO_TARGET_DIR/release/$APP_NAME" >&2
    exit 1
fi

mkdir -p "$ROOT/dist" "$INSTALL_DIR" "$BIN_DIR" "$DESKTOP_DIR" "$PIXMAP_DIR"
# Keep dist/ in sync with whatever we install (so the next run is not stale).
if [[ "$(realpath "$BINARY")" != "$(realpath "$ROOT/dist/$APP_NAME" 2>/dev/null || true)" ]]; then
    install -m 755 "$BINARY" "$ROOT/dist/$APP_NAME"
fi
install -m 755 "$BINARY" "$INSTALL_DIR/$APP_NAME"
ln -sfn "$INSTALL_DIR/$APP_NAME" "$BIN_DIR/$APP_NAME"

install_icon() {
    local src="$1"
    local dest="$2"
    if [[ ! -f "$src" ]]; then
        return 1
    fi
    if command -v magick >/dev/null 2>&1; then
        magick "$src" -resize 512x512^ -gravity center -extent 512x512 "$dest"
    else
        cp "$src" "$dest"
    fi
}

ICON_FILE=""
for icon_candidate in \
    "$ROOT/assets/icon.png" \
    "$ROOT/src-tauri/icons/icon.png" \
    "$ROOT/src-tauri/icons/128x128.png"; do
    if [[ -f "$icon_candidate" ]]; then
        ICON_FILE="$icon_candidate"
        break
    fi
done

DESKTOP_ICON="$PIXMAP_DIR/${APP_NAME}.png"
if [[ -n "$ICON_FILE" ]]; then
    mkdir -p "$ICON_THEME_ROOT/512x512/apps"
    install_icon "$ICON_FILE" "$ICON_THEME_ROOT/512x512/apps/${APP_NAME}.png"
    install_icon "$ICON_FILE" "$DESKTOP_ICON"

    if command -v magick >/dev/null 2>&1; then
        for size in 256 128 64 48 32; do
            mkdir -p "$ICON_THEME_ROOT/${size}x${size}/apps"
            magick "$ICON_FILE" -resize "${size}x${size}" \
                "$ICON_THEME_ROOT/${size}x${size}/apps/${APP_NAME}.png"
        done
    fi

    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$ICON_THEME_ROOT" 2>/dev/null || true
    fi
elif [[ -f "$ICON_THEME_ROOT/512x512/apps/${APP_NAME}.png" ]]; then
    DESKTOP_ICON="$ICON_THEME_ROOT/512x512/apps/${APP_NAME}.png"
fi

cat > "$DESKTOP_DIR/${APP_NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$DISPLAY_NAME
GenericName=Web Spider
Comment=Desktop web spider (Tauri)
Exec=$INSTALL_DIR/$APP_NAME
Icon=$DESKTOP_ICON
Terminal=false
Categories=Development;Network;
StartupWMClass=spider-tauri
EOF

chmod 644 "$DESKTOP_DIR/${APP_NAME}.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

echo "Встановлено: $INSTALL_DIR/$APP_NAME"
echo "Команда в PATH: $BIN_DIR/$APP_NAME"
echo "Меню: $DESKTOP_DIR/${APP_NAME}.desktop"
if [[ -f "$DESKTOP_ICON" ]]; then
    echo "Іконка: $DESKTOP_ICON"
fi
echo "Джерело збірки: $BINARY"
echo "Повторний запуск цього скрипта оновлює встановлену копію."
