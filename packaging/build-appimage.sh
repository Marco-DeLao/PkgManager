#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="$project_root/packaging/AppDir"
output="$project_root/src-tauri/target/release/bundle/appimage/PkgManager_0.1.0_amd64.AppImage"
appimagetool="$project_root/packaging/appimagetool-x86_64.AppImage"

if [[ ! -x "$project_root/src-tauri/target/release/app" ]]; then
	npm --prefix "$project_root" run build
	cargo build --manifest-path "$project_root/src-tauri/Cargo.toml" --release
fi

if [[ ! -x "$appimagetool" ]]; then
	curl -L --fail --retry 3 -o "$appimagetool" \
		https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
	chmod +x "$appimagetool"
fi

rm -rf "$app_dir"
mkdir -p "$app_dir/usr/bin" "$app_dir/usr/share/applications" "$app_dir/usr/share/icons/hicolor/128x128/apps"
install -m 0755 "$project_root/src-tauri/target/release/app" "$app_dir/usr/bin/pkgmanager"
install -m 0644 "$project_root/icon.png" "$app_dir/usr/share/icons/hicolor/128x128/apps/pkgmanager.png"
install -m 0644 "$project_root/icon.png" "$app_dir/.DirIcon"
install -m 0644 "$project_root/icon.png" "$app_dir/pkgmanager.png"

cat > "$app_dir/usr/share/applications/pkgmanager.desktop" <<'DESKTOP'
[Desktop Entry]
Name=PkgManager
Comment=Manage, update, and clean Linux packages
Exec=pkgmanager
Icon=pkgmanager
Terminal=false
Type=Application
Categories=System;Utility;
DESKTOP
install -m 0644 "$app_dir/usr/share/applications/pkgmanager.desktop" "$app_dir/pkgmanager.desktop"

cat > "$app_dir/AppRun" <<'APPRUN'
#!/usr/bin/env bash
set -euo pipefail
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/pkgmanager" "$@"
APPRUN
chmod 0755 "$app_dir/AppRun"

mkdir -p "$(dirname "$output")"
APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool" "$app_dir" "$output"
printf 'Created %s\n' "$output"