# Tie App

A desktop application built with Iced.

## Features

- Opens a single window displaying `hello!`
- Adds a system tray icon; left-click it to hide or restore the window
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

It creates `target/macos/Tie.app` with the app icon configured for Finder, Launchpad,
and the Dock. You can launch it with:

```bash
open target/macos/Tie.app
```

The bundle metadata is in `packaging/macos/Info.plist`. Change its bundle identifier
to a unique reverse-DNS identifier before distributing the app.