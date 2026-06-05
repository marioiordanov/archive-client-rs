Directory watching service with GUI, written in Rust — select a local folder to watch and it will automatically sync changes to Google Drive.

# Usage on macOS

**1. Get the binary** — either download the latest release from [Releases](../../releases) or build from source:

```
./scripts/build_and_sign.sh release
./target/release/archive-client-rs
```

**2. Build the Finder Sync extension** — the extension is not notarized or signed, so it must be built locally. Open `macos/ArchiveClientHost/ArchiveClientHost.xcodeproj` in Xcode, select your development team, and build/run the `ArchiveClientHost` scheme.

# Usage on Windows

Run the installation script — it downloads the latest release and installs the Explorer context menu extension:

```
./install.ps1
```
