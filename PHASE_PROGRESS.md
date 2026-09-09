# PkgManager Phase Progress

## AppImage Packaging

Status: COMPLETE via manual appimagetool fallback

- Tauri's bundled linuxdeploy (`1-alpha`, 2024-07-26) still fails during AppImage bundling on this Arch host.
- Replacing the cached tool with upstream linuxdeploy `1-alpha-20251107-1` and plugin `1-alpha-20250213-1` did not change Tauri's `failed to run linuxdeploy` result.
- Added `packaging/build-appimage.sh`, which constructs an AppDir and packages it with current appimagetool (`2025-12-04` build).
- Verified the generated `PkgManager_0.1.0_amd64.AppImage` runs with `APPIMAGE_EXTRACT_AND_RUN=1`; native window title was `PkgManager` and the UI opened without stderr.
- `.deb` and `.rpm` builds were re-run successfully after the fallback was added.

## Updates, Version-Aware Duplicates, and Orphans

Status: IMPLEMENTED; live-tested on this Arch host

Updates:

- Added an Updates tab with read-only Pacman/Flatpak checks, explicit Update All confirmation, per-package execution, captured stdout/stderr, shared `action=update` audit records, and post-update scan refresh.
- Live check found 116 Pacman updates. `adwaita-fonts` was updated from `50.0-1` to `51.0-2` through the app path with status 0 and a detailed audit record.
- APT/DNF update parsers are fixture-tested only. Flatpak update listing is implemented against `flatpak remote-ls --updates -j`; no live Flatpak updates were available on this host.
- Selective update checkboxes were not implemented; Update All is the current workflow.

Version-aware duplicates:

- Duplicate groups now compare conservative numeric versions, including Pacman epoch/release forms. Non-semantic or ambiguous versions produce no recommendation.
- Live identical-version test: Pacman GIMP `3.2.4-2` plus Flatpak GIMP `3.2.4` showed no newer recommendation; the disposable Flatpak copy was removed afterward.
- Live ambiguous test data is represented by installed platform/runtime entries with non-application version metadata; the matcher fixture also verifies `stable` safely produces no recommendation.
- A genuinely newer real cross-source pair was unavailable because both installed GIMP sources currently reported 3.2.4; the newer recommendation is covered by a focused fixture test rather than claimed as live-proven.

Orphans:

- Added an Orphans tab using live `pacman -Qtdq`, package-size metadata, exact bulk confirmation, shared `action=orphan_remove` audit records, refresh, and re-check.
- Live baseline found 12 existing Pacman orphans; none were mass-removed.
- Disposable cycle: installed `fortune-mod`, removed it, detected newly orphaned `recode`, removed only `recode` through the new path, and verified `pacman -Q recode` failed afterward.
- APT/DNF orphan parsers are fixture-tested only. Flatpak reports no dry-run mode for `uninstall --unused`, so unused Flatpak runtimes are not auto-listed or removed.

Final validation:

- Full Rust suite: 38 library tests passed and 5 duplicate-detection integration tests passed.
- Frontend production build passed.

## Install Tab: Search and Install

Status: IMPLEMENTED; live-tested on this Arch host

Implemented:

- Added distro detection from `/etc/os-release`; Arch exposes Pacman plus available Flatpak, while Debian/Fedora families select APT/DNF respectively.
- Added source-grouped package search in a third `Install` tab. Pacman and Flatpak use live commands; APT and DNF parsers are fixture-tested only.
- Added exact-result validation, identifier sanitization, explicit confirmation previews, non-interactive flags, Polkit execution, stdout/stderr logging, cancellation, audit records with `action=install`, and post-install scan refresh.
- Flatpak installs use explicit `--system` scope through Polkit so the existing system scanner sees the package after refresh.

Live validation:

- Pacman: installed `sl` through the install module with `pkexec pacman -S --noconfirm -- sl`, verified with `pacman -Q sl`, then removed it through the existing cleanup module.
- Flatpak: installed `org.kde.kalk` through the install module with `pkexec flatpak install --system --assumeyes flathub org.kde.kalk`, verified with `flatpak list --system`, then removed it through the existing cleanup module.
- Both successful installs wrote `action=install` records to the shared cleanup audit log. Both test packages were removed afterward.

Known limits:

- APT/DNF install and search adapters are fixture-tested only on this Arch host and need verification on Debian/Fedora.
- Snap support was not added because `snap`/`snapd` is unavailable on this host.
- Flatpak progress output is captured after the process completes; cancellation kills the active package-manager child and the package manager's own transaction semantics apply.

## Phase 5: Privilege Handling and Audit Logging

Status: COMPLETE

Implemented:

- Exact per-item confirmation text: `REMOVE <name>`.
- Core system, kernel, and service package protection.
- Narrow argument-based `pkexec` commands for APT, RPM, Pacman, Flatpak, Snap, and AppImage cleanup.
- No shell interpolation for execution.
- Local append-only audit records under `$XDG_STATE_HOME/pkgclean/cleanup.log` or `~/.local/state/pkgclean/cleanup.log`.
- Frontend confirmation input and cleanup result reporting.

Executed validation:

- `cargo test cleanup::tests`: 5 passed.
- The test suite removed a disposable AppImage fixture through the real `pkexec` binary and verified the audit log entry.
- Wrong confirmation was verified to execute no command.
- Protected package classification was verified.

Host validation:

- `pkexec` version 127 is installed.
- A direct disposable-file command `pkexec /usr/bin/rm -- <temporary-file>` exited with status 0.

## Phase 6: Flatpak Packaging

Status: DEFERRED: KNOWN PACKAGING LIMITATION

Flatpak packaging is intentionally not part of the current standalone verification pass. WebKitGTK portal behavior remains unreliable in the sandbox on this host. Standalone desktop execution and the `.deb`/`.rpm` targets are the supported paths; revisit Flatpak packaging separately.

## Path Picker Portal Regression Fix

Status: COMPLETE

- Added a `Choose folder` control and `pick_scan_path` Tauri command.
- Normal `.deb`/desktop execution uses `kdialog` first and `zenity` second.
- The `ashpd` portal client is reached only when `FLATPAK_ID` or `/.flatpak-info` indicates Flatpak execution.
- Focused Rust tests passed for desktop branch selection and non-portal command construction.
- Rebuilt the Debian bundle successfully.
- Opened the native KDE chooser in the active desktop session for four seconds while monitoring D-Bus. The chooser timed out only because no selection was made; no `org.freedesktop.portal.FileChooser` traffic or portal error was captured.
- Frontend build passed with the new chooser control.
- Flatpak picker regression was fixed after testing the installed app: commit `f38af60d...` now uses `flatpak-spawn --host kdialog` before the portal fallback.
- Direct sandbox chooser test timed out only waiting for user selection, captured zero `org.freedesktop.portal.FileChooser` events, and emitted no portal error.
- Rebuilt from the current workspace as commit `c865b27b284353d98adfb0c88ea3490e0e8947c49d947a8851be053b9c9273b3` and force-installed it with `flatpak --user install --reinstall`.
- `flatpak run com.pkgclean.desktop` produced empty stdout/stderr and remained alive for 12 seconds (`timeout` exit 124).
- The exact installed commit's chooser fallback produced empty stdout/stderr, remained open for 5 seconds awaiting selection (`timeout` exit 124), and generated zero portal FileChooser events.
- Removed the portal fallback entirely for the Arch Flatpak build. Current installed commit `22ff39a043d99020779b2231152c1c06ebb45c7eb3e096d79125f4eb89e9931c` has no portal permissions, starts with empty stderr, and its host `kdialog` chooser test produced zero portal events.
