#!/bin/sh
# ABOUTME: Builds cbr-egui in release mode and installs it as /Applications/cbr-egui.app.
# ABOUTME: Registers the bundle with LaunchServices and makes it the default opener for .cbz/.cbr.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="/Applications/cbr-egui.app"
BUNDLE_ID="dev.jsh.cbr-egui"
VERSION="0.1.0"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

# xcrun hands swift the highest-versioned SDK in the developer directory, which
# may be an orphan left behind by a newer toolchain than the installed compiler.
# swift then refuses to build the Swift module. Fall back to the newest SDK this
# compiler does accept.
if ! swift -e '' >/dev/null 2>&1; then
    for sdk in $(ls -d "$(dirname "$(xcrun --show-sdk-path)")"/MacOSX*.sdk | sort -rV); do
        if SDKROOT="$sdk" swift -e '' >/dev/null 2>&1; then
            export SDKROOT="$sdk"
            echo "==> Default SDK is newer than the installed Swift compiler; using $sdk"
            break
        fi
    done
    if [ -z "${SDKROOT:-}" ]; then
        echo "error: no macOS SDK works with $(swift --version 2>&1 | tail -1)" >&2
        exit 1
    fi
fi

echo "==> Building release binary"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Assembling $APP"
STAGING="$(mktemp -d)/cbr-egui.app"
mkdir -p "$STAGING/Contents/MacOS" "$STAGING/Contents/Resources"
cp "$ROOT/target/release/cbr-egui" "$STAGING/Contents/MacOS/cbr-egui"

echo "==> Generating icon from assets/cbr-egui.png"
ICONSET="$(mktemp -d)/cbr-egui.iconset"
mkdir -p "$ICONSET"
# The source art is not square; aspect-fit it onto square transparent canvases.
swift - "$ROOT/assets/cbr-egui.png" "$ICONSET" <<'SWIFT'
import AppKit

let args = CommandLine.arguments
guard let src = NSImage(contentsOfFile: args[1]) else {
    fatalError("cannot read icon source \(args[1])")
}
let outDir = URL(fileURLWithPath: args[2])

func writeIcon(pixels: Int, name: String) {
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    ) else { fatalError("cannot create bitmap rep") }
    rep.size = NSSize(width: pixels, height: pixels)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let scale = min(CGFloat(pixels) / src.size.width, CGFloat(pixels) / src.size.height)
    let w = src.size.width * scale
    let h = src.size.height * scale
    let rect = NSRect(x: (CGFloat(pixels) - w) / 2, y: (CGFloat(pixels) - h) / 2, width: w, height: h)
    src.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1.0)
    NSGraphicsContext.restoreGraphicsState()
    guard let png = rep.representation(using: .png, properties: [:]) else {
        fatalError("cannot encode png")
    }
    try! png.write(to: outDir.appendingPathComponent(name))
}

for size in [16, 32, 128, 256, 512] {
    writeIcon(pixels: size, name: "icon_\(size)x\(size).png")
    writeIcon(pixels: size * 2, name: "icon_\(size)x\(size)@2x.png")
}
SWIFT
iconutil -c icns "$ICONSET" -o "$STAGING/Contents/Resources/cbr-egui.icns"

cat > "$STAGING/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>cbr-egui</string>
    <key>CFBundleIconFile</key>
    <string>cbr-egui</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>cbr-egui</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Comic Book ZIP Archive</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>CFBundleTypeIconFile</key>
            <string>cbr-egui</string>
            <key>LSHandlerRank</key>
            <string>Owner</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>cbz</string>
            </array>
        </dict>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Comic Book RAR Archive</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>CFBundleTypeIconFile</key>
            <string>cbr-egui</string>
            <key>LSHandlerRank</key>
            <string>Owner</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>cbr</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

codesign --force --sign - "$STAGING"

rm -rf "$APP"
mv "$STAGING" "$APP"

echo "==> Registering with LaunchServices"
# Drop stale registrations of the cargo-run dev shim bundles, which share the
# bundle id but have no document types, so Finder resolves to this install.
for dev_bundle in "$ROOT/target/debug/cbr-egui.app" "$ROOT/target/release/cbr-egui.app"; do
    if [ -d "$dev_bundle" ]; then
        "$LSREGISTER" -u "$dev_bundle" || true
    fi
done
"$LSREGISTER" -f "$APP"

echo "==> Setting default handler for .cbz and .cbr"
swift - "$APP" <<'SWIFT'
import AppKit
import UniformTypeIdentifiers

let appURL = URL(fileURLWithPath: CommandLine.arguments[1])
let group = DispatchGroup()
for ext in ["cbz", "cbr"] {
    guard let utype = UTType(filenameExtension: ext) else {
        print("\(ext): could not resolve a type identifier")
        continue
    }
    group.enter()
    NSWorkspace.shared.setDefaultApplication(at: appURL, toOpen: utype) { error in
        if let error {
            print("\(ext): failed to set default handler: \(error.localizedDescription)")
        } else {
            print("\(ext): default handler set (\(utype.identifier))")
        }
        group.leave()
    }
}
group.wait()
for ext in ["cbz", "cbr"] {
    if let utype = UTType(filenameExtension: ext),
       let handler = NSWorkspace.shared.urlForApplication(toOpen: utype) {
        print("\(ext) now opens with: \(handler.path)")
    }
}
SWIFT

echo "==> Done. Installed $APP"
