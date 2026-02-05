use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::io::Cursor;
use clap::Parser;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::ImageFormat;
use tiny_http::{Server, Response, Header};

#[derive(Parser, Debug)]
#[command(name = "Rust Browser Claude")]
#[command(about = "Desktop browser with live streaming capability")]
struct Args {
    /// Run in headless mode (no GUI, uses Chrome)
    #[arg(long)]
    headless: bool,

    /// Initial URL to load
    #[arg(long, default_value = "https://example.com")]
    url: String,

    /// HTTP server port for live stream
    #[arg(long, default_value = "8765")]
    port: u16,

    /// Viewport width (headless mode)
    #[arg(long, default_value = "1200")]
    width: u32,

    /// Viewport height (headless mode)
    #[arg(long, default_value = "800")]
    height: u32,
}

// ============== Shared Types ==============

type ScreenshotBuffer = Arc<Mutex<Option<Vec<u8>>>>;
type CurrentUrl = Arc<Mutex<String>>;

// ============== HTTP Server ==============

fn start_http_server_headless(
    port: u16,
    screenshot_buffer: ScreenshotBuffer,
    current_url: CurrentUrl,
) {
    thread::spawn(move || {
        let addr = format!("0.0.0.0:{}", port);
        let server = match Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to start HTTP server: {}", e);
                return;
            }
        };

        println!("Live stream: http://localhost:{}/live-stream", port);
        println!("Viewer:      http://localhost:{}/", port);

        for request in server.incoming_requests() {
            let url = request.url();

            if url == "/live-stream" {
                let buffer = screenshot_buffer.lock().unwrap();
                if let Some(ref jpeg_bytes) = *buffer {
                    let base64_frame = BASE64.encode(jpeg_bytes);
                    let current = current_url.lock().unwrap().clone();
                    let json = serde_json::json!({
                        "frame": base64_frame,
                        "url": current,
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                    });

                    let response = Response::from_string(json.to_string())
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                        .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                        .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap());
                    let _ = request.respond(response);
                } else {
                    let response = Response::from_string(r#"{"error":"no frame available"}"#)
                        .with_status_code(503)
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                    let _ = request.respond(response);
                }
            } else if url.starts_with("/navigate?") {
                // Navigate to URL: /navigate?url=https://example.com
                if let Some(new_url) = url.strip_prefix("/navigate?url=") {
                    let decoded = urlencoding::decode(new_url).unwrap_or_default();
                    *current_url.lock().unwrap() = decoded.to_string();
                    let response = Response::from_string(r#"{"status":"navigating"}"#)
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                    let _ = request.respond(response);
                } else {
                    let response = Response::from_string(r#"{"error":"missing url parameter"}"#)
                        .with_status_code(400);
                    let _ = request.respond(response);
                }
            } else if url == "/" {
                let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Rust Browser Claude - Live Stream</title>
    <style>
        body { margin: 0; background: #1a1a1a; display: flex; flex-direction: column; align-items: center; min-height: 100vh; padding: 20px; box-sizing: border-box; }
        #controls { display: flex; gap: 10px; margin-bottom: 10px; width: 100%; max-width: 1200px; }
        #url-input { flex: 1; padding: 8px 12px; border-radius: 4px; border: none; font-size: 14px; }
        #go-btn { padding: 8px 16px; background: #4a90d9; color: white; border: none; border-radius: 4px; cursor: pointer; }
        #go-btn:hover { background: #3a80c9; }
        img { max-width: 100%; max-height: calc(100vh - 100px); border: 1px solid #333; }
        #status { position: fixed; top: 10px; right: 10px; color: #0f0; font-family: monospace; background: rgba(0,0,0,0.7); padding: 5px 10px; border-radius: 4px; }
        #current-url { color: #888; font-family: monospace; font-size: 12px; margin-bottom: 10px; }
    </style>
</head>
<body>
    <div id="controls">
        <input type="text" id="url-input" placeholder="Enter URL..." />
        <button id="go-btn">Go</button>
    </div>
    <div id="current-url">-</div>
    <div id="status">Connecting...</div>
    <img id="screen" />
    <script>
        const img = document.getElementById('screen');
        const status = document.getElementById('status');
        const currentUrlEl = document.getElementById('current-url');
        const urlInput = document.getElementById('url-input');
        const goBtn = document.getElementById('go-btn');
        let frameCount = 0;

        async function navigate(url) {
            if (!url.startsWith('http')) url = 'https://' + url;
            await fetch('/navigate?url=' + encodeURIComponent(url));
        }

        goBtn.onclick = () => navigate(urlInput.value);
        urlInput.onkeydown = (e) => { if (e.key === 'Enter') navigate(urlInput.value); };

        async function fetchFrame() {
            try {
                const response = await fetch('/live-stream');
                const data = await response.json();

                if (data.frame) {
                    img.src = 'data:image/jpeg;base64,' + data.frame;
                    frameCount++;
                    status.textContent = 'Frames: ' + frameCount;
                    if (data.url) {
                        currentUrlEl.textContent = data.url;
                        urlInput.value = data.url;
                    }
                }
            } catch (e) {
                status.textContent = 'Error: ' + e.message;
            }

            setTimeout(fetchFrame, 100);
        }

        status.textContent = 'Connected';
        fetchFrame();
    </script>
</body>
</html>"#;

                let response = Response::from_string(html)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
                let _ = request.respond(response);
            } else {
                let response = Response::from_string("Not Found").with_status_code(404);
                let _ = request.respond(response);
            }
        }
    });
}

// ============== Headless Mode (Chrome CDP) ==============

async fn run_headless(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    use chromiumoxide::browser::{Browser, BrowserConfig};
    use futures::StreamExt;

    println!("Starting headless browser...");

    let screenshot_buffer: ScreenshotBuffer = Arc::new(Mutex::new(None));
    let current_url: CurrentUrl = Arc::new(Mutex::new(args.url.clone()));

    // Start HTTP server
    start_http_server_headless(args.port, screenshot_buffer.clone(), current_url.clone());

    // Launch headless Chrome
    let config = BrowserConfig::builder()
        .window_size(args.width, args.height)
        .build()
        .map_err(|e| format!("Failed to build browser config: {}", e))?;

    let (browser, mut handler) = Browser::launch(config).await?;

    // Spawn browser handler
    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            let _ = event;
        }
    });

    // Create page and navigate
    let page = browser.new_page(&args.url).await?;

    println!("Headless browser started!");
    println!("Initial URL: {}", args.url);
    println!("");
    println!("Navigate via: http://localhost:{}/navigate?url=<URL>", args.port);

    let mut last_url = args.url.clone();

    // Main loop: capture screenshots and handle navigation
    loop {
        // Check if URL changed (via HTTP API)
        let new_url = current_url.lock().unwrap().clone();
        if new_url != last_url {
            println!("Navigating to: {}", new_url);
            if let Err(e) = page.goto(&new_url).await {
                eprintln!("Navigation error: {}", e);
            }
            last_url = new_url;
        }

        // Wait for page to be ready
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Capture screenshot
        match page.screenshot(
            chromiumoxide::page::ScreenshotParams::builder()
                .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Jpeg)
                .quality(80)
                .build()
        ).await {
            Ok(png_data) => {
                *screenshot_buffer.lock().unwrap() = Some(png_data);
            }
            Err(e) => {
                eprintln!("Screenshot error: {}", e);
            }
        }

        // Small delay between captures
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[allow(unreachable_code)]
    {
        _handle.abort();
        Ok(())
    }
}

// ============== GUI Mode (wry/tao) ==============

mod gui {
    use super::*;
    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        window::WindowBuilder,
    };
    use wry::WebViewBuilder;

    #[derive(Debug, Clone)]
    pub struct Tab {
        pub id: usize,
        pub url: String,
        pub title: String,
    }

    #[derive(Debug, Clone)]
    pub enum UserEvent {
        Navigate(String),
        NewTab,
        CloseTab(usize),
        SwitchTab(usize),
        PageLoaded,
    }

    pub type Tabs = Arc<Mutex<(Vec<Tab>, usize, usize)>>;
    pub type WindowRect = Arc<Mutex<(i32, i32, u32, u32)>>;

    pub const INIT_SCRIPT: &str = r#"
window.__rustBrowserReady = true;
window.__injectToolbar = function(tabsHtml, currentUrl) {
    const old = document.getElementById('__rust_browser_toolbar__');
    if (old) old.remove();

    if (!document.body) {
        setTimeout(function() { window.__injectToolbar(tabsHtml, currentUrl); }, 50);
        return;
    }

    let oldStyle = document.getElementById('__rb_style__');
    if (oldStyle) oldStyle.remove();

    const style = document.createElement('style');
    style.id = '__rb_style__';
    style.textContent = `
        #__rust_browser_toolbar__ {
            position: fixed !important;
            top: 0 !important;
            left: 0 !important;
            right: 0 !important;
            height: 72px !important;
            background: #e8e8e8 !important;
            border-bottom: 1px solid #b0b0b0 !important;
            z-index: 2147483647 !important;
            font-family: -apple-system, BlinkMacSystemFont, sans-serif !important;
            box-sizing: border-box !important;
            display: flex !important;
            flex-direction: column !important;
        }
        .tab-bar {
            display: flex !important;
            align-items: center !important;
            padding: 4px 8px !important;
            gap: 2px !important;
            background: #d0d0d0 !important;
            height: 32px !important;
        }
        .tab {
            display: flex !important;
            align-items: center !important;
            padding: 4px 8px !important;
            background: #c0c0c0 !important;
            border-radius: 6px 6px 0 0 !important;
            cursor: pointer !important;
            font-size: 12px !important;
            max-width: 150px !important;
            gap: 4px !important;
        }
        .tab:hover { background: #d0d0d0 !important; }
        .tab.active { background: #e8e8e8 !important; }
        .tab-title {
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            white-space: nowrap !important;
        }
        .tab-close {
            font-size: 14px !important;
            width: 16px !important;
            height: 16px !important;
            display: flex !important;
            align-items: center !important;
            justify-content: center !important;
            border-radius: 50% !important;
            cursor: pointer !important;
        }
        .tab-close:hover { background: rgba(0,0,0,0.1) !important; }
        .new-tab-btn {
            width: 24px !important;
            height: 24px !important;
            border: none !important;
            background: transparent !important;
            cursor: pointer !important;
            font-size: 18px !important;
            color: #666 !important;
        }
        .new-tab-btn:hover { color: #000 !important; }
        .nav-bar {
            display: flex !important;
            align-items: center !important;
            padding: 4px 8px !important;
            gap: 6px !important;
            height: 40px !important;
        }
        .nav-bar button {
            width: 28px !important;
            height: 26px !important;
            border: 1px solid #a0a0a0 !important;
            border-radius: 4px !important;
            background: linear-gradient(to bottom, #fff, #e8e8e8) !important;
            cursor: pointer !important;
            font-size: 14px !important;
        }
        .nav-bar button:hover { background: linear-gradient(to bottom, #fff, #d8d8d8) !important; }
        .nav-bar input {
            flex: 1 !important;
            height: 26px !important;
            border: 1px solid #a0a0a0 !important;
            border-radius: 13px !important;
            padding: 0 12px !important;
            font-size: 12px !important;
            outline: none !important;
            background: white !important;
        }
        .nav-bar input:focus { border-color: #4a90d9 !important; }
        html { margin-top: 72px !important; }
    `;
    document.head.appendChild(style);

    const toolbar = document.createElement('div');
    toolbar.id = '__rust_browser_toolbar__';
    toolbar.innerHTML = `
        <div class="tab-bar">
            ${tabsHtml}
            <button class="new-tab-btn" id="__rb_newtab__" title="New Tab">+</button>
        </div>
        <div class="nav-bar">
            <button id="__rb_back__" title="Back">←</button>
            <button id="__rb_fwd__" title="Forward">→</button>
            <button id="__rb_reload__" title="Reload">⟳</button>
            <input type="text" id="__rb_url__" value="${currentUrl}" placeholder="Enter URL...">
        </div>
    `;

    document.body.insertBefore(toolbar, document.body.firstChild);

    document.getElementById('__rb_back__').onclick = function() { history.back(); };
    document.getElementById('__rb_fwd__').onclick = function() { history.forward(); };
    document.getElementById('__rb_reload__').onclick = function() { location.reload(); };

    const urlInput = document.getElementById('__rb_url__');
    urlInput.onkeydown = function(e) {
        if (e.key === 'Enter') {
            window.ipc.postMessage(JSON.stringify({navigate: urlInput.value.trim()}));
        }
    };
    urlInput.onfocus = function() { this.select(); };

    document.getElementById('__rb_newtab__').onclick = function() {
        window.ipc.postMessage(JSON.stringify({newTab: true}));
    };

    document.querySelectorAll('.tab').forEach(function(tab) {
        tab.onclick = function(e) {
            if (!e.target.classList.contains('tab-close')) {
                window.ipc.postMessage(JSON.stringify({switchTab: parseInt(tab.dataset.id)}));
            }
        };
    });

    document.querySelectorAll('.tab-close').forEach(function(btn) {
        btn.onclick = function(e) {
            e.stopPropagation();
            window.ipc.postMessage(JSON.stringify({closeTab: parseInt(btn.dataset.id)}));
        };
    });
};

window.ipc.postMessage(JSON.stringify({pageLoaded: true}));

document.addEventListener('keydown', function(e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'l') {
        e.preventDefault();
        const urlInput = document.getElementById('__rb_url__');
        if (urlInput) { urlInput.focus(); urlInput.select(); }
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 't') {
        e.preventDefault();
        window.ipc.postMessage(JSON.stringify({newTab: true}));
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'w') {
        e.preventDefault();
        window.ipc.postMessage(JSON.stringify({closeCurrentTab: true}));
    }
});
"#;

    pub fn build_tabs_html(tabs: &[(usize, String, String)], active_id: usize) -> String {
        tabs.iter().map(|(id, title, _url)| {
            let active_class = if *id == active_id { "active" } else { "" };
            let short_title = if title.len() > 18 {
                format!("{}...", &title[..15])
            } else {
                title.clone()
            };
            format!(
                r#"<div class="tab {}" data-id="{}"><span class="tab-title">{}</span><span class="tab-close" data-id="{}">×</span></div>"#,
                active_class, id, short_title, id
            )
        }).collect()
    }

    pub fn inject_toolbar_script(tabs_html: &str, current_url: &str) -> String {
        format!(
            r#"if (window.__injectToolbar) {{ window.__injectToolbar(`{}`, `{}`); }}"#,
            tabs_html.replace('`', "\\`"),
            current_url.replace('`', "\\`")
        )
    }

    fn capture_window(window_rect: &WindowRect) -> Option<Vec<u8>> {
        use screenshots::Screen;

        let (x, y, width, height) = *window_rect.lock().ok()?;

        if width == 0 || height == 0 {
            return None;
        }

        let screens = Screen::all().ok()?;
        let screen = screens.first()?;

        let capture = screen.capture_area(x, y, width, height).ok()?;

        let rgba_image = image::RgbaImage::from_raw(
            capture.width(),
            capture.height(),
            capture.to_vec(),
        )?;

        let rgb_image = image::DynamicImage::ImageRgba8(rgba_image).to_rgb8();

        let mut jpeg_bytes = Cursor::new(Vec::new());
        rgb_image.write_to(&mut jpeg_bytes, ImageFormat::Jpeg).ok()?;

        Some(jpeg_bytes.into_inner())
    }

    fn start_http_server_gui(port: u16, screen_changed: Arc<AtomicBool>, window_rect: WindowRect) {
        thread::spawn(move || {
            let addr = format!("0.0.0.0:{}", port);
            let server = match Server::http(&addr) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to start HTTP server: {}", e);
                    return;
                }
            };

            for request in server.incoming_requests() {
                let url = request.url();

                if url == "/live-stream" {
                    screen_changed.store(false, Ordering::Relaxed);

                    if let Some(jpeg_bytes) = capture_window(&window_rect) {
                        let base64_frame = BASE64.encode(&jpeg_bytes);
                        let json = serde_json::json!({
                            "frame": base64_frame,
                            "timestamp": std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        });

                        let response = Response::from_string(json.to_string())
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap());
                        let _ = request.respond(response);
                    } else {
                        let response = Response::from_string(r#"{"error":"capture failed"}"#)
                            .with_status_code(500)
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                        let _ = request.respond(response);
                    }
                } else if url == "/" {
                    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Rust Browser Claude - Live Stream</title>
    <style>
        body { margin: 0; background: #1a1a1a; display: flex; justify-content: center; align-items: center; min-height: 100vh; }
        img { max-width: 100%; max-height: 100vh; }
        #status { position: fixed; top: 10px; left: 10px; color: #0f0; font-family: monospace; background: rgba(0,0,0,0.7); padding: 5px 10px; border-radius: 4px; }
    </style>
</head>
<body>
    <div id="status">Connecting...</div>
    <img id="screen" />
    <script>
        const img = document.getElementById('screen');
        const status = document.getElementById('status');
        let frameCount = 0;

        async function fetchFrame() {
            try {
                const response = await fetch('/live-stream');
                const data = await response.json();

                if (data.frame) {
                    img.src = 'data:image/jpeg;base64,' + data.frame;
                    frameCount++;
                    status.textContent = 'Frames: ' + frameCount;
                }
            } catch (e) {
                status.textContent = 'Error: ' + e.message;
            }

            setTimeout(fetchFrame, 100);
        }

        status.textContent = 'Connected';
        fetchFrame();
    </script>
</body>
</html>"#;

                    let response = Response::from_string(html)
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap());
                    let _ = request.respond(response);
                } else {
                    let response = Response::from_string("Not Found").with_status_code(404);
                    let _ = request.respond(response);
                }
            }
        });
    }

    pub fn run_gui(args: Args) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();

        let window = WindowBuilder::new()
            .with_title("Rust Browser Claude")
            .with_inner_size(LogicalSize::new(args.width as f64, args.height as f64))
            .build(&event_loop)?;

        let screen_changed = Arc::new(AtomicBool::new(true));
        let window_rect: WindowRect = Arc::new(Mutex::new((0, 0, args.width, args.height)));

        if let Ok(pos) = window.outer_position() {
            let size = window.outer_size();
            *window_rect.lock().unwrap() = (pos.x, pos.y, size.width, size.height);
        }

        start_http_server_gui(args.port, screen_changed.clone(), window_rect.clone());

        let tabs: Tabs = Arc::new(Mutex::new((
            vec![Tab { id: 1, url: args.url.clone(), title: "New Tab".to_string() }],
            1,
            2,
        )));

        let tabs_ipc = tabs.clone();
        let proxy_ipc = proxy.clone();
        let screen_changed_ipc = screen_changed.clone();

        let webview = WebViewBuilder::new()
            .with_url(&args.url)
            .with_initialization_script(INIT_SCRIPT)
            .with_ipc_handler(move |req| {
                let body = req.body();
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) {
                    if let Some(url) = msg["navigate"].as_str() {
                        let _ = proxy_ipc.send_event(UserEvent::Navigate(url.to_string()));
                    }
                    if msg["newTab"].as_bool() == Some(true) {
                        let _ = proxy_ipc.send_event(UserEvent::NewTab);
                    }
                    if let Some(id) = msg["switchTab"].as_u64() {
                        let _ = proxy_ipc.send_event(UserEvent::SwitchTab(id as usize));
                    }
                    if let Some(id) = msg["closeTab"].as_u64() {
                        let _ = proxy_ipc.send_event(UserEvent::CloseTab(id as usize));
                    }
                    if msg["closeCurrentTab"].as_bool() == Some(true) {
                        let (_, active_id, _) = &*tabs_ipc.lock().unwrap();
                        let _ = proxy_ipc.send_event(UserEvent::CloseTab(*active_id));
                    }
                    if msg["pageLoaded"].as_bool() == Some(true) {
                        let _ = proxy_ipc.send_event(UserEvent::PageLoaded);
                    }
                }
                screen_changed_ipc.store(true, Ordering::Relaxed);
            })
            .with_devtools(true)
            .build(&window)?;

        println!("Rust Browser Claude started (GUI mode)");
        println!("Cmd+T: New tab | Cmd+W: Close tab | Cmd+L: Focus URL | F12: DevTools");
        println!("");
        println!("Live stream: http://localhost:{}/live-stream", args.port);
        println!("Viewer:      http://localhost:{}/", args.port);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match &event {
                Event::WindowEvent { .. } | Event::UserEvent(_) => {
                    screen_changed.store(true, Ordering::Relaxed);
                }
                _ => {}
            }

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,

                Event::WindowEvent {
                    event: WindowEvent::Moved(position),
                    ..
                } => {
                    let mut rect = window_rect.lock().unwrap();
                    rect.0 = position.x;
                    rect.1 = position.y;
                }

                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    let mut rect = window_rect.lock().unwrap();
                    rect.2 = size.width;
                    rect.3 = size.height;
                }

                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput { event: key_event, .. },
                    ..
                } => {
                    if key_event.state == tao::event::ElementState::Pressed {
                        if let tao::keyboard::KeyCode::F12 = key_event.physical_key {
                            if webview.is_devtools_open() {
                                webview.close_devtools();
                            } else {
                                webview.open_devtools();
                            }
                        }
                    }
                }

                Event::UserEvent(ref user_event) => {
                    match user_event {
                        UserEvent::PageLoaded => {
                            let (tabs_vec, active_id, _) = &*tabs.lock().unwrap();
                            let current_url = tabs_vec.iter()
                                .find(|t| t.id == *active_id)
                                .map(|t| t.url.as_str())
                                .unwrap_or("about:blank");
                            let tabs_data: Vec<_> = tabs_vec.iter()
                                .map(|t| (t.id, t.title.clone(), t.url.clone()))
                                .collect();
                            let tabs_html = build_tabs_html(&tabs_data, *active_id);
                            let script = inject_toolbar_script(&tabs_html, current_url);
                            let _ = webview.evaluate_script(&script);
                        }

                        UserEvent::Navigate(url) => {
                            let url = if !url.starts_with("http://") && !url.starts_with("https://") {
                                if url.contains('.') && !url.contains(' ') {
                                    format!("https://{}", url)
                                } else {
                                    format!("https://www.google.com/search?q={}", url.replace(' ', "+"))
                                }
                            } else {
                                url.clone()
                            };

                            {
                                let (tabs_vec, active_id, _) = &mut *tabs.lock().unwrap();
                                if let Some(tab) = tabs_vec.iter_mut().find(|t| t.id == *active_id) {
                                    tab.url = url.clone();
                                    if let Ok(parsed) = url::Url::parse(&url) {
                                        tab.title = parsed.host_str().unwrap_or("Page").to_string();
                                    }
                                }
                            }

                            let js = format!("window.location.href = '{}'", url.replace('\'', "\\'"));
                            let _ = webview.evaluate_script(&js);
                        }

                        UserEvent::NewTab => {
                            let new_url = "https://example.com".to_string();
                            {
                                let (tabs_vec, active_id, next_id) = &mut *tabs.lock().unwrap();
                                let new_tab = Tab {
                                    id: *next_id,
                                    url: new_url.clone(),
                                    title: "New Tab".to_string(),
                                };
                                tabs_vec.push(new_tab);
                                *active_id = *next_id;
                                *next_id += 1;
                            }

                            let js = format!("window.location.href = '{}'", new_url);
                            let _ = webview.evaluate_script(&js);
                        }

                        UserEvent::CloseTab(id) => {
                            let should_navigate: Option<String>;
                            {
                                let (tabs_vec, active_id, _) = &mut *tabs.lock().unwrap();
                                if tabs_vec.len() <= 1 {
                                    return;
                                }

                                let idx = tabs_vec.iter().position(|t| t.id == *id);
                                if let Some(idx) = idx {
                                    tabs_vec.remove(idx);

                                    if *active_id == *id {
                                        let new_idx = idx.min(tabs_vec.len() - 1);
                                        *active_id = tabs_vec[new_idx].id;
                                        should_navigate = Some(tabs_vec[new_idx].url.clone());
                                    } else {
                                        should_navigate = None;
                                    }
                                } else {
                                    should_navigate = None;
                                }
                            }

                            if let Some(url) = should_navigate {
                                let js = format!("window.location.href = '{}'", url.replace('\'', "\\'"));
                                let _ = webview.evaluate_script(&js);
                            } else {
                                let (tabs_vec, active_id, _) = &*tabs.lock().unwrap();
                                let current_url = tabs_vec.iter()
                                    .find(|t| t.id == *active_id)
                                    .map(|t| t.url.as_str())
                                    .unwrap_or("about:blank");
                                let tabs_data: Vec<_> = tabs_vec.iter()
                                    .map(|t| (t.id, t.title.clone(), t.url.clone()))
                                    .collect();
                                let tabs_html = build_tabs_html(&tabs_data, *active_id);
                                let script = inject_toolbar_script(&tabs_html, current_url);
                                let _ = webview.evaluate_script(&script);
                            }
                        }

                        UserEvent::SwitchTab(id) => {
                            let url: String;
                            {
                                let (tabs_vec, active_id, _) = &mut *tabs.lock().unwrap();
                                if let Some(tab) = tabs_vec.iter().find(|t| t.id == *id) {
                                    *active_id = *id;
                                    url = tab.url.clone();
                                } else {
                                    return;
                                }
                            }

                            let js = format!("window.location.href = '{}'", url.replace('\'', "\\'"));
                            let _ = webview.evaluate_script(&js);
                        }
                    }
                }

                _ => {}
            }
        });
    }
}

// ============== Main ==============

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.headless {
        // Run headless mode with tokio runtime
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(run_headless(args))
    } else {
        // Run GUI mode
        gui::run_gui(args)
    }
}
