use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::io::Cursor;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;
use tiny_http::{Server, Response, Header};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::ImageFormat;

#[derive(Debug, Clone)]
struct Tab {
    id: usize,
    url: String,
    title: String,
}

#[derive(Debug, Clone)]
enum UserEvent {
    Navigate(String),
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    PageLoaded,
    ScreenChanged,
}

type Tabs = Arc<Mutex<(Vec<Tab>, usize, usize)>>;
type ScreenBuffer = Arc<Mutex<Option<Vec<u8>>>>; // Latest screenshot as JPEG bytes
type WindowRect = Arc<Mutex<(i32, i32, u32, u32)>>; // x, y, width, height

const INIT_SCRIPT: &str = r#"
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

fn build_tabs_html(tabs: &[(usize, String, String)], active_id: usize) -> String {
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

fn inject_toolbar_script(tabs_html: &str, current_url: &str) -> String {
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

    // Capture the specific area
    let capture = screen.capture_area(x, y, width, height).ok()?;

    // Convert to JPEG
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

fn start_http_server(_screen_buffer: ScreenBuffer, screen_changed: Arc<AtomicBool>, window_rect: WindowRect) {
    thread::spawn(move || {
        let server = match Server::http("0.0.0.0:8765") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to start HTTP server: {}", e);
                return;
            }
        };

        println!("Live stream available at http://localhost:8765/live-stream");

        for request in server.incoming_requests() {
            let url = request.url();

            if url == "/live-stream" {
                // Return single frame as JSON
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
                // Simple HTML viewer with polling
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
        let lastTimestamp = 0;

        async function fetchFrame() {
            try {
                const response = await fetch('/live-stream');
                const data = await response.json();

                if (data.frame && data.timestamp !== lastTimestamp) {
                    img.src = 'data:image/jpeg;base64,' + data.frame;
                    lastTimestamp = data.timestamp;
                    frameCount++;
                    status.textContent = 'Frames: ' + frameCount;
                }
            } catch (e) {
                status.textContent = 'Error: ' + e.message;
            }

            setTimeout(fetchFrame, 100); // Poll every 100ms
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Rust Browser Claude")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .build(&event_loop)?;

    // Screen streaming state
    let screen_buffer: ScreenBuffer = Arc::new(Mutex::new(None));
    let screen_changed = Arc::new(AtomicBool::new(true));

    // Window position and size for capturing
    let window_rect: WindowRect = Arc::new(Mutex::new((0, 0, 1200, 800)));

    // Update initial window rect
    if let Ok(pos) = window.outer_position() {
        let size = window.outer_size();
        *window_rect.lock().unwrap() = (pos.x, pos.y, size.width, size.height);
    }

    // Start HTTP server for live streaming
    start_http_server(screen_buffer.clone(), screen_changed.clone(), window_rect.clone());

    // Initialize tabs
    let tabs: Tabs = Arc::new(Mutex::new((
        vec![Tab { id: 1, url: "https://example.com".to_string(), title: "Example".to_string() }],
        1,
        2,
    )));

    let tabs_ipc = tabs.clone();
    let proxy_ipc = proxy.clone();
    let screen_changed_ipc = screen_changed.clone();

    let webview = WebViewBuilder::new()
        .with_url("https://example.com")
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
            // Mark screen as changed on any IPC message
            screen_changed_ipc.store(true, Ordering::Relaxed);
        })
        .with_devtools(true)
        .build(&window)?;

    println!("Rust Browser Claude started!");
    println!("Cmd+T: New tab | Cmd+W: Close tab | Cmd+L: Focus URL");
    println!("");
    println!("Live stream: http://localhost:8765/live-stream");
    println!("Viewer:      http://localhost:8765/");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Mark screen changed on any window event
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
                    UserEvent::PageLoaded | UserEvent::ScreenChanged => {
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
