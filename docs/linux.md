# Glim on Linux

This is a Linux port of [Glim](https://github.com/glimapp/glim), not a rewrite. The GPUI workspace, tabs, file tree, text editor and Rust readers are retained. It is a development build, not yet a feature-equivalent Linux release.

## Build and install

Use a current Rust toolchain with edition 2024 support, a C compiler, Python 3, pkg-config, and development libraries for Wayland, X11/XCB, xkbcommon, fontconfig, FreeType, OpenSSL, SQLite, libarchive and Vulkan. The desktop needs a working GPU driver, or an appropriate software Vulkan implementation. File dialogs use the desktop portal.

Arch Linux/CachyOS dependencies:

```sh
pkexec pacman -S --needed base-devel rust pkgconf python wayland libxcb libxkbcommon libxkbcommon-x11 fontconfig freetype2 openssl sqlite libarchive vulkan-icd-loader poppler glib2 xdg-utils xdg-desktop-portal
```

Install the portal backend appropriate to your desktop separately, for example the Hyprland backend plus a file-chooser backend. Other distributions need the equivalent development packages. Python is used by build-time generators and the Linux binary-plist reader. Poppler provides `pdfinfo` and `pdftoppm`; GLib provides `gio trash`. No WebKitGTK or Swift installation is required.

```sh
./scripts/dev build --locked
./scripts/dev run --locked -- /path/to/folder
./scripts/dev run --locked -- /path/to/file.pdf
./scripts/install-linux --debug
```

For an optimized build, run `./scripts/install-linux` without `--debug`. The installer places `glim` in `~/.local/bin` and a launcher/icon under `${XDG_DATA_HOME:-~/.local/share}`. It does not change default file associations. The debug installation is suitable for trying the port; optimized packaging has not been validated.

The `scripts/dev` wrapper selects the distribution's pkg-config when available, so a Homebrew Rust installation can find system development libraries.

## Current support

- GPUI workspace, folder tree, tabs, file search and session persistence.
- Source editing and Markdown, JSON/tree, CSV/table, SQLite, archive, ebook and other existing Rust readers.
- Common raster images, GIFs and sanitized SVGs, without Apple thumbnail tools. Working-memory budgets still apply. ICNS and Apple-only image codecs have not been ported.
- PDF raster previews with previous/next page controls. One page is rendered per background job; changing tabs cancels pending work. PDF text selection, search, zoom, password entry and PDFKit-style navigation are not available yet.
- Linux context menus, reader toolbar, path tooltips and desktop Trash. Trash failures never fall back to permanent deletion.
- Linux modifier names in welcome actions and the shortcuts reference. Application bindings use GPUI's cross-platform `secondary` modifier, which is Ctrl on Linux.

Office, audio/video, embedded HTML and OpenAPI native readers currently offer an external-app fallback. HTML initially opens in source mode; an external preview of unsaved HTML does not preserve relative resource paths. Mermaid and other WebKit-rendered diagrams remain unavailable. Finder integration, macOS default-application settings and Sparkle updates are not Linux features. macOS binaries/releases are not Linux updates.

Do not associate an unsupported format with Glim as its default application: its external-app fallback would reopen Glim. Use a specialist application for those formats. SSH askpass and macOS-specific editor/MCP launch integrations have not been ported or verified.

Wayland startup has been checked on CachyOS/Hyprland. X11, other desktops, portal dialogs, remote connections, clipboard interoperability and all individual format interactions still need manual acceptance. macOS compatibility has not been rebuilt from Linux.

## Local data

- State database and preferences: `${XDG_DATA_HOME:-~/.local/share}/glim/glim.sqlite3`.
- Diagnostic logs: `${XDG_STATE_HOME:-~/.local/state}/glim/logs`.

The fork retains upstream's privacy and telemetry settings. Review them in Settings before use. Do not use this development build on the only copy of an important editable document: upstream source editing can save automatically.

## Checks

```sh
./scripts/dev check --workspace --all-targets --locked
./scripts/dev fmt --all -- --check
./scripts/dev clippy --workspace --all-targets --locked -- -D warnings
./scripts/dev test --workspace --locked
./scripts/dev build --locked
```

The PDF tests require Poppler. The macOS code-signing integration test is compiled only on macOS. EPUB fixtures are assembled with the existing Rust ZIP dependency rather than a system `zip` executable.

See [architecture](architecture.md#linux-platform-boundary) and [agent instructions](../AGENTS.md).
