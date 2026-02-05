# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Desktop browser for macOS with remote live streaming capability. Built with Rust using wry (WebView) and tao (window management).

Target platform: macOS (uses WKWebView under the hood)

## Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run debug build
```

## Architecture

**Single-process model using wry WebView:**
- Window management via `tao` crate
- WebView rendering via `wry` crate (WKWebView on macOS)
- HTTP server for live streaming via `tiny_http`
- Screenshot capture via `screenshots` crate

**Tech stack:**
- Language: Rust (edition 2021)
- Web engine: wry (WKWebView)
- Window: tao
- HTTP: tiny_http
- Image processing: image, base64

**Key components in main.rs:**
- `Tab` struct - tab state (id, url, title)
- `UserEvent` enum - IPC events between JS and Rust
- `INIT_SCRIPT` - JavaScript injected into every page for toolbar UI
- `capture_window()` - captures window area as JPEG
- `start_http_server()` - HTTP server for `/live-stream` endpoint

## Key Implementation Details

- Toolbar (tabs, navigation, URL bar) is injected via JavaScript on every page load
- IPC between JS and Rust via `window.ipc.postMessage()` with JSON payloads
- Tab switching navigates the single WebView to different URLs (not true multi-tab)
- Live stream available at `http://localhost:8765/live-stream` (JSON with base64 JPEG frame)
- Viewer page at `http://localhost:8765/` polls for frames

## Keyboard Shortcuts

- `Cmd+T` - New tab
- `Cmd+W` - Close current tab
- `Cmd+L` - Focus URL bar
- `F12` - Toggle DevTools
