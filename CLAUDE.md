# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Desktop browser for macOS built with Rust and CEF (Chromium Embedded Framework). The project aims to create a developer-focused browser with native desktop UX, web-view tabs, sidebar panel, and DOM/JavaScript interaction capabilities.

Target platform: macOS (ARM64 + x86_64)

## Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run debug build
cargo run --release      # Run release build
```

## Architecture

**Multi-process model:**
- **Browser Process (Rust)** - window management, tab management, user input, navigation, JS execution requests
- **Render Process (CEF)** - JavaScript execution, DOM building, page rendering

**Tech stack:**
- Language: Rust (edition 2024)
- Web engine: CEF
- UI: Cocoa (NSWindow, NSSplitView) or winit
- DOM parsing: html5ever / scraper

**Planned module structure:**
```
src/
├── main.rs              # Entry point
├── app/
│   ├── window.rs        # Window management (NSWindow)
│   ├── tabs.rs          # Tab management (each tab = CefBrowser)
│   ├── sidebar.rs       # Side panel (separate CefBrowser instance)
│   └── navigation.rs    # Back/Forward navigation
├── cef/
│   ├── init.rs          # CEF initialization
│   ├── browser.rs       # CefBrowser wrapper
│   └── handlers.rs      # Event handlers (OnLoadEnd, etc.)
└── dom/
    └── parser.rs        # HTML parsing via html5ever/scraper
```

## Key Implementation Details

- Each tab is a separate `CefBrowser` instance
- Single `CefRequestContext` shared across all tabs
- Side panel uses its own `CefBrowser` embedded as child-window
- DOM access via JS: `document.documentElement.outerHTML` or `CefFrame::GetSource`
- JS execution only after `OnLoadEnd` callback
- Navigation state checked via `browser.can_go_back()` / `browser.can_go_forward()`

## Technical Plan

Full technical specification is available in `src/main.md`.
