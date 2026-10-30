#!/usr/bin/env bash
# Build Retold Studio and wrap it as a native installer for the current platform.
# Usage: scripts/package.sh   (writes installers into dist/)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$root/experiments/retold-desktop/Cargo.toml"
target="$root/target/package"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$manifest" | head -1)"
dist="$root/dist"
mkdir -p "$dist"
cargo build --release --locked --manifest-path "$manifest" --target-dir "$target" --bin retold-studio
binary="$target/release/retold-studio"
arch="$(uname -m)"

case "$(uname -s)" in
  Darwin)
    app="$dist/Retold Studio.app"
    rm -rf "$app" "$dist/dmg"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/retold-studio"
    sed "s/__VERSION__/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
    if [ -f "$root/packaging/macos/Retold.icns" ]; then
      cp "$root/packaging/macos/Retold.icns" "$app/Contents/Resources/Retold.icns"
    fi
    mkdir -p "$dist/dmg"
    cp -R "$app" "$dist/dmg/"
    ln -s /Applications "$dist/dmg/Applications"
    # hdiutil's own size estimate runs short on CI runners; leave explicit headroom.
    size_mb=$(( $(du -sm "$dist/dmg" | cut -f1) + 64 ))
    hdiutil create -volname "Retold Studio" -srcfolder "$dist/dmg" -size "${size_mb}m" -ov -format UDZO \
      "$dist/Retold-Studio-$version-macos-$arch.dmg"
    rm -rf "$dist/dmg"
    ;;
  Linux)
    stage="$dist/retold-studio-$version-linux-$arch"
    rm -rf "$stage"
    mkdir -p "$stage/usr/bin" "$stage/usr/share/applications" "$stage/DEBIAN"
    cp "$binary" "$stage/usr/bin/retold-studio"
    strip "$stage/usr/bin/retold-studio" || true
    cp "$root/packaging/linux/retold-studio.desktop" "$stage/usr/share/applications/"
    mkdir -p "$stage/usr/share/icons/hicolor/512x512/apps"
    cp "$root/packaging/linux/retold-studio.png" "$stage/usr/share/icons/hicolor/512x512/apps/"
    deb_arch="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
    cat > "$stage/DEBIAN/control" <<CONTROL
Package: retold-studio
Version: $version
Section: video
Priority: optional
Architecture: $deb_arch
Depends: ffmpeg, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libvulkan1, libfontconfig1, libasound2 | libasound2t64
Maintainer: Retold contributors <noreply@github.com>
Description: Turn any story into a narrated recap video.
CONTROL
    dpkg-deb --build --root-owner-group "$stage" "$dist/retold-studio_${version}_${deb_arch}.deb"
    tar -C "$stage/usr" -czf "$dist/retold-studio-$version-linux-$arch.tar.gz" bin share
    rm -rf "$stage"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    cp "$binary.exe" "$dist/retold-studio.exe"
    # Inno Setup is a native Windows tool; hand it Windows paths, not MSYS ones.
    win_dist=$(cygpath -w "$dist")
    iscc //DVersion="$version" //DSource="$win_dist\\retold-studio.exe" //O"$win_dist" "$(cygpath -w "$root/packaging/windows/retold-studio.iss")"
    ;;
  *) echo "Unsupported platform: $(uname -s)" >&2; exit 1 ;;
esac
ls -la "$dist"
