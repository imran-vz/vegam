# Avoid Paid Platform Signing for V1

Production v1 does not require paid platform signing certificates. The macOS `.dmg` and Windows `.msi` may ship unsigned to avoid Apple Developer Program and Windows code-signing certificate costs; Linux `.AppImage` releases can use free integrity or signing mechanisms. This means macOS Gatekeeper and Windows SmartScreen warnings are expected v1 release friction and must be explained in release/download instructions.
