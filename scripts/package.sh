#!/bin/bash
set -euo pipefail

if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--install" ) ]]; then
    echo "用法：scripts/package.sh [--install]" >&2
    exit 2
fi

project_dir="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
cd "$project_dir"

if command -v cargo >/dev/null; then
    cargo_bin="$(command -v cargo)"
else
    cargo_bin="$HOME/.cargo/bin/cargo"
fi

export MACOSX_DEPLOYMENT_TARGET=14.0
export CARGO_TARGET_DIR="$project_dir/target"
"$cargo_bin" build --release --locked

app_name="$(plutil -extract CFBundleDisplayName raw assets/Info.plist)"
bundle="$project_dir/dist/$app_name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
install -m 0755 target/release/shuidi "$bundle/Contents/MacOS/shuidi"
install -m 0644 assets/Info.plist "$bundle/Contents/Info.plist"
install -m 0644 assets/AppIcon.icns "$bundle/Contents/Resources/AppIcon.icns"
plutil -lint "$bundle/Contents/Info.plist"
codesign --force --sign - "$bundle"
codesign --verify --strict "$bundle"
printf '%s\n' "$bundle"

if [[ $# -eq 1 ]]; then
    destination="$HOME/Applications/$app_name.app"
    legacy="$HOME/Applications/水滴.app"
    install_path="$destination"
    if [[ -e "$destination" ]]; then
        installed_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$destination/Contents/Info.plist" 2>/dev/null || true)"
        if [[ "$installed_id" != "local.shuidi.focus" ]]; then
            printf '已存在同名应用，未覆盖 %s\n' "$destination" >&2
            exit 1
        fi
    elif [[ -d "$legacy" ]]; then
        legacy_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$legacy/Contents/Info.plist" 2>/dev/null || true)"
        if [[ "$legacy_id" == "local.shuidi.focus" ]]; then
            install_path="$legacy"
        fi
    fi
    mkdir -p "$HOME/Applications"
    ditto "$bundle" "$install_path"
    codesign --verify --strict "$install_path"
    if [[ "$install_path" != "$destination" ]]; then
        mv "$install_path" "$destination"
    fi
    printf '%s\n' "$destination"
fi
