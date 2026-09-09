<div align="center">

<img src="assets/banner.svg" alt="PkgManager banner" width="100%" />

<br/><br/>

[![License: MIT](https://img.shields.io/badge/License-MIT-3b82f6.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux-3b82f6.svg)](#-supported-distributions)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20%2B%20Rust-3b82f6.svg)](https://tauri.app)

**A unified desktop app for managing packages across every package manager on your Linux system — in one place.**


</div>

---

##  Features

| Tab | What it does |
|---|---|
| **All Packages** | A single, searchable, sortable list of every package installed on your system, from every source combined. |
| **Duplicates** | Detects the same application installed through more than one package manager, shows the wasted space, and highlights which copy is the newer version. |
| **Install** | Search for and install new packages from Pacman/APT/DNF or Flatpak, with results ranked by relevance and grouped by source. |
| **Updates** | Check for available updates across every source and apply them all in one batch, with a clear per-package success/failure report. |
| **Orphans** | Finds leftover dependency packages that nothing on your system needs anymore, and lets you clean them up safely. |

**Built-in safety, on every action:**
-  Every install, update, or removal requires an explicit, typed confirmation — no accidental one-click destructive actions.
-  Privileged operations go through **Polkit**, never running the whole app as root.
-  A local **audit log** records every change made — what was installed or removed, when, and how.
-  No telemetry, no network calls beyond talking to your package managers — everything runs locally.

##  Download

The easiest way to run PkgManager is the AppImage — a single file, no installation required.

1. Go to the [Releases](../../releases) page and download the latest `PkgManager-x86_64.AppImage`.
2. Make it executable and run it:
   ```bash
   chmod +x PkgManager-x86_64.AppImage
   ./PkgManager-x86_64.AppImage
   ```
   If it doesn't launch, your system may be missing `FUSE`, or you can run it in extract mode instead:
   ```bash
   ./PkgManager-x86_64.AppImage --appimage-extract-and-run
   ```

## 🐧 Supported Distributions

PkgManager works with any distribution built on one of these package manager families:

| Family | Distributions | Status |
|---|---|---|
| **Pacman** (Arch-based) | Arch Linux, Manjaro, EndeavourOS, Garuda Linux, ArcoLinux, RebornOS, Artix Linux, and others | ✅ Fully tested |
| **APT** (Debian-based) | Ubuntu, Debian, Linux Mint, Pop!_OS, Zorin OS, elementary OS, Kali Linux, Raspberry Pi OS, MX Linux, Linux Lite, Deepin, and others | ✅ Fully tested |
| **DNF/RPM** (Fedora-based) | Fedora, RHEL, Rocky Linux, AlmaLinux, CentOS Stream, Nobara, openSUSE (via RPM compatibility), and others | ✅ Fully tested |
| **Flatpak** | Any distribution with Flatpak installed | ✅ Fully tested |
| **Snap** | Any distribution with Snap installed | ⚠️ Implemented, limited testing |

Package scanning, searching, installing, updating, and orphan detection have all been verified against real Pacman, APT, and DNF systems. If you run into an issue on your specific distribution, please [open an issue](../../issues).

## 🔐 Security

Security was treated as a first-class concern throughout development:

- All system commands are built using safe argument arrays — never raw shell strings — preventing command injection.
- Package identifiers are validated against real search/scan results before any action runs.
- Every destructive action requires the user to type an exact confirmation phrase before it executes.
- Privileged operations request elevation through Polkit for only the specific command that needs it — the app itself never runs as root.
- A local, append-only audit log records every install, update, and removal.

If you discover a security issue, please open an issue or reach out directly rather than disclosing it publicly.

## 🛠 Build from Source

**Prerequisites:** [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) (v18+), and the [Tauri system dependencies](https://tauri.app/start/prerequisites/) for your distribution.

```bash
git clone https://github.com/Marco-DeLao/pkgmanager.git
cd pkgmanager
npm install
cargo tauri dev      # run in development mode
```

To produce a release build:

```bash
cargo tauri build --bundles deb,rpm
```

To build the AppImage (uses a manual `appimagetool`-based script, since Tauri's default AppImage bundler has known compatibility issues on some systems):

```bash
./packaging/build-appimage.sh
```

## 🧱 Tech Stack

- **Backend:** Rust + [Tauri](https://tauri.app)
- **Frontend:** HTML / CSS / JavaScript
- **Packaging:** `.deb`, `.rpm`, AppImage


## 🤝 Contributing

Issues and pull requests are welcome. If you're fixing a bug or adding a feature, please include a short description of how you tested it.

---
