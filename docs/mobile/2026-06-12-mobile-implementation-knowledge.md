# Mobile Implementation Knowledge (Preserved for the Mobile Release)

Date: 2026-06-12

Per ADR `0001`, mobile (Android/iOS) is removed from the Desktop Release production
path. This note preserves the hard-to-recover knowledge from the working Android
prototype and the generated iOS scaffolding before deletion, so the future Mobile
Release does not start from zero. It documents what existed at commit `efbecd2`
and earlier; the implementation itself can be recovered from git history.

## Android File Access (content:// URIs)

Android does not hand apps plain file paths from pickers; it hands `content://`
URIs that must be read through the Storage Access Framework.

- Dependency: `tauri-plugin-android-fs` (no crates.io release used; pinned to
  `git = "https://github.com/aiueo13/tauri-plugin-android-fs", branch = "main"`),
  registered only on Android via
  `[target.'cfg(target_os = "android")'.dependencies]`.
- Reading pattern (was `src-tauri/src/platform.rs`):
  1. If the path starts with `content://`, parse it with `url::Url::parse` and
     wrap as `tauri_plugin_fs::FilePath::Url(url)`; otherwise use
     `FilePath::Path(path.into())`.
  2. Convert `FilePath` into `tauri_plugin_android_fs::FileUri` (infallible
     `From` conversion).
  3. `app.android_fs_async().open_file_readable(&uri).await` returns a readable
     file; `read_to_end` into a buffer.
- The desktop/mobile split was a single `platform::read_file(app, path)`
  function with `#[cfg(target_os = "android")]` / `#[cfg(not(...))]` variants,
  so callers never branched on platform.
- Known limitation: this read the whole file into memory, which already failed
  the 100 GB requirement. A future mobile path must stream from the SAF file
  descriptor instead.

## QR Ticket Scanning

Mobile used the camera to scan a Transfer Ticket QR code rendered by the Sender.

- Frontend: `@tauri-apps/plugin-barcode-scanner` (`~2.4.2`), with
  `scan({ windowed: true, formats: [Format.QRCode] })`.
- The `windowed: true` trick: the camera renders *behind* the webview, so the
  Tauri window must be transparent for the preview to show through. This is why
  `tauri.conf.json` had `"transparent": true` on the main window. Without a
  transparent window the scanner appears as a black screen.
- `cancel()` from the same plugin stops an in-progress scan.
- Rust side: `tauri-plugin-barcode-scanner = "2"` registered only for
  `cfg(any(target_os = "android", target_os = "ios"))`.
- Required capability permissions (was `src-tauri/capabilities/mobile.json`,
  `platforms: ["android", "iOS"]`):
  `barcode-scanner:allow-scan`, `allow-cancel`, `allow-request-permissions`,
  `allow-check-permissions`.
- The Sender displayed the ticket as a QR via `qrcode.react` (`QRCodeSVG`,
  size 200, error-correction level "M"). Long encrypted tickets produce dense
  QR codes; level "M" still scanned reliably on phones.
- `@capacitor/camera` was present in `package.json` but unused; do not bring it
  back for the Mobile Release.

## Android Build, Signing, and Release

The `Makefile` encoded a working signed-APK release flow:

- One-time setup: `keytool -genkey -v -keystore upload-keystore.jks -keyalg RSA
  -keysize 2048 -validity 10000 -alias upload`.
- Release builds wrote `src-tauri/gen/android/keystore.properties` with
  `password=...`, `keyAlias=upload`, `storeFile=<abs path to .jks>` before
  `pnpm tauri android build`, then deleted the properties file afterwards so
  secrets never lingered in the repo.
- Signed APK output path:
  `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`.
- Device install used `adb install -r` on that signed release APK (the
  Makefile's `android-install` depended on `android-release`; its "Installing
  debug APK" echo was a mislabel). There was no true debug-APK target —
  `android-debug` ran a plain `pnpm tauri android build` (release-mode,
  unsigned).
- `.gitignore` excluded `*.keystore`, `*.jks`, `keystore.properties`.
- SDK levels from the checked-in project: `tauri.conf.json` set
  `bundle.android.minSdkVersion: 28`, and `app/build.gradle.kts` set
  `compileSdk = 36` and `targetSdk = 36`, so the build host needed Android SDK
  Platform 36 plus the NDK. (The old README's "SDK 33+" understated this.)
- The Android project scaffolding under `src-tauri/gen/android/` started from
  `pnpm tauri android init` output but was hand-customized afterwards — do NOT
  assume a regenerated project is equivalent. The hand edits beyond Gradle
  keystore consumption are listed in the next section.

## Android Manifest and Project Customizations

`src-tauri/gen/android/app/src/main/AndroidManifest.xml` was hand-edited from
the generated template:

- Extra `uses-permission` entries beyond `INTERNET`: `CAMERA` (required by the
  barcode scanner), `READ_EXTERNAL_STORAGE`, `WRITE_EXTERNAL_STORAGE`,
  `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE`, and
  `CHANGE_WIFI_MULTICAST_STATE` (needed for Iroh local-network discovery).
- The template's `android:usesCleartextTraffic="${usesCleartextTraffic}"` was
  replaced with `android:networkSecurityConfig="@xml/network_security_config"`
  pointing at a hand-written `res/xml/network_security_config.xml`: cleartext
  disabled globally (system trust anchors only) plus an explicit HTTPS
  domain-config for the Iroh relay domains `iroh.computer`, `iroh.network`,
  and `n0.computer` (all with subdomains).
- An `androidx.core.content.FileProvider` declaration with authority
  `${applicationId}.fileprovider`, `grantUriPermissions="true"`, backed by
  `res/xml/file_paths.xml` (an `external-path` and a `cache-path`, both `.`).
- `MainActivity.kt` overrode `onCreate` to call
  `androidx.activity.enableEdgeToEdge()` before `super.onCreate(...)` — the
  native half of the edge-to-edge layout whose web half is
  `viewport-fit=cover`.

## iOS Scaffolding

- `src-tauri/gen/apple/` held the `pnpm tauri ios init` output: Xcode project,
  `project.yml`, `Podfile`, `ExportOptions.plist`, asset catalogs, and
  `vegam_iOS/Info.plist`.
- `src-tauri/Info.ios.plist` (the Tauri-specific overlay plist, not part of
  the generated Xcode project) existed solely to add
  `NSCameraUsageDescription` = "Read QR codes" — iOS kills the app at runtime
  when the barcode scanner opens the camera without this string. A future iOS
  build must restore it.
- No iOS-specific Rust or TS code existed beyond the barcode-scanner plugin
  registration; iOS never reached a working build in this repo.

## Mobile-Specific Runtime Configuration

- Tauri entry point used `#[cfg_attr(mobile, tauri::mobile_entry_point)]` on
  `vegam_lib::run()`; required for mobile, harmless on desktop.
- The Android Tauri builder differed from desktop:
  - extra plugins: `tauri_plugin_barcode_scanner`, `tauri_plugin_android_fs`;
  - log level `Debug` (desktop used `Info`);
  - an extra `Target::new(TargetKind::Webview)` log target so Rust logs reached
    the WebView console, which is the practical way to see them on a device.
- Mobile networks establish relay connections slower than desktop; the Iroh
  init code waited ~5 s after bind before reading the node address for relay
  URLs, sized for mobile radio wake-up. Desktop can use event-driven waits.
- Android file pickers return display names, not paths; the ticket metadata
  (filename embedded in the enhanced ticket) mattered more on Android because
  the receiver could not infer a filename from a picked output path.

## Frontend Responsive Patterns

The UI was written mobile-first: `viewport-fit=cover` in `index.html`, and
`md:` Tailwind breakpoints throughout so the same layout worked on phones.
These are retained in the desktop app and need no recovery.
