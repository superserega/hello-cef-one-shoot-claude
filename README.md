## Rust Browser Claude

Браузер на Rust с возможностью удаленного livestream.

### Режимы запуска

**GUI режим** (с окном, macOS):
```bash
cargo run -- --url https://example.com
```

**Headless режим** (без GUI, требуется Chrome):
```bash
cargo run -- --headless --url https://example.com --port 8765
```

### CLI аргументы

| Аргумент | По умолчанию | Описание |
|----------|--------------|----------|
| `--headless` | false | Запуск без GUI через Chrome |
| `--url <URL>` | https://example.com | Начальный URL |
| `--port <PORT>` | 8765 | Порт HTTP сервера |
| `--width <W>` | 1200 | Ширина viewport |
| `--height <H>` | 800 | Высота viewport |

### HTTP API

| Endpoint | Описание |
|----------|----------|
| `GET /` | Веб-вьювер с live stream |
| `GET /live-stream` | JSON: `{"frame": "<base64 JPEG>", "url": "...", "timestamp": ...}` |
| `GET /navigate?url=<URL>` | Навигация на URL (headless режим) |

### Техстек

- **Rust** (edition 2021)
- **wry** - WebView (GUI режим)
- **chromiumoxide** - Chrome CDP (headless режим)
- **tiny_http** - HTTP сервер

---

*Проект полностью разработан Claude Code*

*src/main.md - план разработки (сделано в ChatGPT)*
