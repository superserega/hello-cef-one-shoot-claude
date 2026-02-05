# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Desktop browser for macOS with remote live streaming capability. Supports two modes:
- **GUI mode** - native window with wry/WKWebView
- **Headless mode** - no GUI, uses Chrome via CDP (Chrome DevTools Protocol)

## Build & Run

```bash
cargo build              # Debug build
cargo build --release    # Release build

# GUI mode (default)
cargo run -- --url https://example.com

# Headless mode (no GUI, requires Chrome)
cargo run -- --headless --url https://example.com --port 8765
```

## CLI Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `--headless` | false | Run without GUI using headless Chrome |
| `--url <URL>` | https://example.com | Initial URL to load |
| `--port <PORT>` | 8765 | HTTP server port for live stream |
| `--width <W>` | 1200 | Viewport width |
| `--height <H>` | 800 | Viewport height |

## Architecture

**GUI Mode (wry):**
- Window management via `tao` crate
- WebView rendering via `wry` crate (WKWebView on macOS)
- Screenshot capture via `screenshots` crate

**Headless Mode (chromiumoxide):**
- Chrome browser controlled via CDP
- Native screenshot via `Page.captureScreenshot`
- No display required

**Shared:**
- HTTP server via `tiny_http` for live streaming
- JSON API for frame delivery and navigation

## HTTP API

| Endpoint | Description |
|----------|-------------|
| `GET /` | Web viewer with live stream display |
| `GET /live-stream` | JSON: `{"frame": "<base64 JPEG>", "url": "...", "timestamp": ...}` |
| `GET /navigate?url=<URL>` | Navigate to URL (headless mode) |

## Keyboard Shortcuts (GUI mode)

- `Cmd+T` - New tab
- `Cmd+W` - Close current tab
- `Cmd+L` - Focus URL bar
- `F12` - Toggle DevTools
