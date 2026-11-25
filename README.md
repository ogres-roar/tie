# Tie App

A desktop application built with Iced.

## Features

- Opens a single window displaying `hello!`
- Uses a custom title bar with a toggle for keeping the window always on top
- Adds a system tray icon; left-click it to restore the window and return to its desktop
- Sets a custom macOS Dock icon
- Cross-platform compatibility (macOS, Windows, Linux)

## Getting Started

### Prerequisites

- A recent Rust toolchain
- Cargo (Rust package manager)

### Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd tie
```

2. Build the application:
```bash
cargo build
```

3. Run the application:
```bash
cargo run
```

On Linux, building the system tray support requires GTK 3 and AppIndicator development
libraries (for example, `libgtk-3-dev` and `libappindicator3-dev` on Debian/Ubuntu).
The Dock icon artwork is in `assets/tie-icon.svg`; the app embeds its rendered PNG.

## Drawer layout

At startup, the app reads `conf/layout.toml` relative to the current working directory
and parses each `[[item]]` into a drawer. Use `conf/layout.example.toml` as a template.
Supported item types are `okx` (with `apiKey`, `secretKey`, and the required OKX API
`passphrase`) and `text` (with `content`). The real `conf/layout.toml` is git-ignored
to help avoid committing API credentials.

The app queries `GET /api/v5/account/balance` on startup and every five seconds. It
displays the account's total equity in USD and the USD spot price for each asset whose
USD equity exceeds 1 USD, along with the time of the last successful refresh. Failed
refreshes leave the last successful result visible. A missing or invalid config
prevents the app from starting.

## Building for Release

To create a release build:
```bash
cargo build --release
```

The compiled application will be available in `target/release/tie` (on macOS).

### Package as a macOS app

Run the packaging script on macOS:

```bash
sh scripts/package-macos.sh
```

It copies `conf/layout.toml` into the app's `Contents/Resources/conf` directory.
The app loads this bundled config when running from the `.app`; `cargo run` continues
to load `conf/layout.toml` from the project directory.

The config contains OKX API credentials in plaintext. Anyone who can inspect or copy
the `.app` can retrieve them, so only bundle credentials for a private app you control;
do not distribute that `.app`. For a distributable app, keep credentials outside the
bundle and load them from a per-user config or secure storage.

The package script creates `target/macos/Tie.app` with its icon configured for Finder,
Launchpad, and the Dock. You can launch it with:

```bash
open target/macos/Tie.app
```

The bundle metadata is in `packaging/macos/Info.plist`. Change its bundle identifier
to a unique reverse-DNS identifier before distributing the app.