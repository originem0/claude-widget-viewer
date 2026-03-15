/// HTML shell generation.
/// Sidebar layout for widget history, OnceLock caching, CSP enforcement.

use std::sync::OnceLock;

const DESIGN_SYSTEM_CSS: &str = include_str!("../assets/design-system.css");
const MORPHDOM_JS: &str = include_str!("../assets/morphdom.min.js");

/// Shell layout CSS — sidebar + content area.
/// Uses only variables defined in design-system.css.
const SHELL_CSS: &str = r#"
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    display: flex;
    height: 100vh;
    overflow: hidden;
    font-family: var(--font-sans);
    font-size: 16px;
    line-height: 1.7;
    font-weight: 400;
    color: var(--color-text-primary);
    background: var(--color-bg-primary);
}
#sidebar {
    width: 160px;
    min-width: 160px;
    border-right: 0.5px solid var(--color-border);
    background: var(--color-bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}
#sidebar-header {
    padding: 12px 14px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-secondary);
}
#sidebar-list {
    flex: 1;
    overflow-y: auto;
}
.sidebar-item {
    padding: 7px 14px;
    cursor: pointer;
    font-size: 13px;
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-left: 2px solid transparent;
    transition: background 0.15s;
}
.sidebar-item:hover {
    background: var(--color-border-light);
}
.sidebar-item.active {
    color: var(--color-text-primary);
    border-left-color: var(--color-blue);
    background: var(--color-bg-primary);
    font-weight: 500;
}
#content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 20px;
}
#widget-root {
    width: 100%;
    overflow: hidden;
}
#widget-root > * {
    max-width: 100%;
}
#widget-root > svg {
    width: 100%;
    height: auto;
    display: block;
}
#widget-root canvas {
    max-width: 100%;
}
#widget-root img {
    max-width: 100%;
    height: auto;
}
#widget-root table {
    width: 100%;
}
#loading {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    color: var(--color-text-secondary);
    font-size: 14px;
}
"#;

/// Shell JavaScript — sidebar management, widget injection, hot-reload.
/// Kept as a const to avoid format! brace escaping.
const SHELL_JS: &str = r#"
(function() {
    var widgets = [];
    var activeIndex = -1;

    window.injectWidget = function(b64, title, file) {
        var html = decodeBase64(b64);
        if (!html) return;

        // Dedup: if file path matches an existing entry, update it instead of adding
        if (file) {
            for (var i = 0; i < widgets.length; i++) {
                if (widgets[i].file === file) {
                    widgets[i].html = html;
                    widgets[i].title = title || widgets[i].title;
                    renderSidebar();
                    if (i === activeIndex) {
                        var root = document.getElementById('widget-root');
                        root.innerHTML = html;
                        executeScripts(root);
                        setTimeout(resizeCharts, 100);
                    }
                    return;
                }
            }
        }

        var id = widgets.length;
        widgets.push({ file: file || null, title: title || 'Widget ' + (id + 1), html: html });
        renderSidebar();
        switchTo(id);
    };

    window.updateWidget = function(b64) {
        var html = decodeBase64(b64);
        if (!html) return;
        if (activeIndex < 0 || activeIndex >= widgets.length) return;

        widgets[activeIndex].html = html;
        var root = document.getElementById('widget-root');
        var temp = document.createElement('div');
        temp.innerHTML = html;
        morphdom(root, temp, {
            childrenOnly: true,
            onBeforeElUpdated: function(fromEl, toEl) {
                if (fromEl === document.activeElement) return false;
                return true;
            }
        });
        executeScripts(root);
        setTimeout(resizeCharts, 100);
    };

    function decodeBase64(b64) {
        var bin = atob(b64);
        var bytes = new Uint8Array(bin.length);
        for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        return new TextDecoder('utf-8').decode(bytes);
    }

    function renderSidebar() {
        var list = document.getElementById('sidebar-list');
        list.innerHTML = '';
        widgets.forEach(function(w, i) {
            var item = document.createElement('div');
            item.className = 'sidebar-item' + (i === activeIndex ? ' active' : '');
            item.textContent = w.title;
            item.title = w.title;
            item.onclick = function() { switchTo(i); };
            list.appendChild(item);
        });
        list.scrollTop = list.scrollHeight;
    }

    function switchTo(index) {
        if (index < 0 || index >= widgets.length) return;
        activeIndex = index;
        var w = widgets[index];
        var root = document.getElementById('widget-root');
        root.innerHTML = w.html;
        executeScripts(root);
        setTimeout(resizeCharts, 100);

        var items = document.querySelectorAll('.sidebar-item');
        items.forEach(function(el, i) {
            el.className = 'sidebar-item' + (i === activeIndex ? ' active' : '');
        });

        document.title = w.title + ' \u2014 Claude Widget';
    }

    function executeScripts(container) {
        var scripts = container.querySelectorAll('script');
        scripts.forEach(function(oldScript) {
            var newScript = document.createElement('script');
            Array.from(oldScript.attributes).forEach(function(attr) {
                newScript.setAttribute(attr.name, attr.value);
            });
            if (oldScript.src) {
                newScript.src = oldScript.src;
            } else {
                newScript.textContent = oldScript.textContent;
            }
            oldScript.parentNode.replaceChild(newScript, oldScript);
        });
    }

    window.sendPrompt = function(text) {
        console.log('[sendPrompt stub]', text);
        if (window.ipc) {
            window.ipc.postMessage(JSON.stringify({ type: 'sendPrompt', text: text }));
        }
    };

    // Resize all chart instances when container size changes.
    // Chart.js ResizeObserver doesn't always fire in WebView2.
    // Also dispatches 'widget-resize' for non-Chart.js libraries.
    function resizeCharts() {
        if (window.Chart) {
            Object.values(Chart.instances).forEach(function(c) {
                try { c.resize(); } catch(e) { console.warn('Chart resize failed:', e); }
            });
        }
        window.dispatchEvent(new Event('widget-resize'));
    }
    var resizeTimer = null;
    window.addEventListener('resize', function() {
        if (resizeTimer) clearTimeout(resizeTimer);
        resizeTimer = setTimeout(resizeCharts, 50);
    });
    var root = document.getElementById('widget-root');
    if (root && typeof ResizeObserver !== 'undefined') {
        new ResizeObserver(function() {
            if (resizeTimer) clearTimeout(resizeTimer);
            resizeTimer = setTimeout(resizeCharts, 50);
        }).observe(root);
    }

    // Process pre-loaded widget batch from initialization script.
    // The init script sets window.__WIDGET_BATCH (a data array) before the page loads.
    if (window.__WIDGET_BATCH) {
        window.__WIDGET_BATCH.forEach(function(w) {
            injectWidget(w.b, w.t, w.f);
        });
        delete window.__WIDGET_BATCH;
    }
})();
"#;

static SHELL_HTML: OnceLock<String> = OnceLock::new();

/// Cached shell HTML. Called once per process; subsequent calls return the same &str.
pub fn generate_shell() -> &'static str {
    SHELL_HTML.get_or_init(|| {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' https://cdn.jsdelivr.net https://cdnjs.cloudflare.com https://unpkg.com; style-src 'unsafe-inline' https://cdn.jsdelivr.net https://cdnjs.cloudflare.com https://unpkg.com https://fonts.googleapis.com; img-src data: blob:; font-src https://fonts.gstatic.com https://cdn.jsdelivr.net https://cdnjs.cloudflare.com; connect-src 'none';">
<title>Claude Widget</title>
<style>
{design_system}
{shell_css}
</style>
<script>
{morphdom}
</script>
</head>
<body>
<div id="sidebar">
    <div id="sidebar-header">History</div>
    <div id="sidebar-list"></div>
</div>
<div id="content">
    <div id="widget-root">
        <div id="loading">Waiting for widget...</div>
    </div>
</div>
<script>
{shell_js}
</script>
</body>
</html>"#,
            design_system = DESIGN_SYSTEM_CSS,
            shell_css = SHELL_CSS,
            morphdom = MORPHDOM_JS,
            shell_js = SHELL_JS,
        )
    })
}

/// Generate JS to inject a new widget into the sidebar + content area.
/// `file` is used for dedup — if a widget with the same file path exists, it's updated.
pub fn make_inject_js(html: &str, title: &str, file: Option<&str>) -> String {
    let b64 = base64_encode(html.as_bytes());
    let title_escaped = title.replace('\\', "\\\\").replace('\'', "\\'");
    let file_js = match file {
        Some(f) => {
            let f_escaped = f.replace('\\', "\\\\").replace('\'', "\\'");
            format!("'{}'", f_escaped)
        }
        None => "null".to_string(),
    };
    format!(
        "{{var _b='{}';var _t='{}';var _f={};if(typeof injectWidget==='function'){{injectWidget(_b,_t,_f)}}else{{document.addEventListener('DOMContentLoaded',function(){{injectWidget(_b,_t,_f)}})}}}}",
        b64, title_escaped, file_js
    )
}

/// Generate JS to hot-reload the active widget's HTML (morphdom patch).
pub fn make_update_js(html: &str) -> String {
    let b64 = base64_encode(html.as_bytes());
    format!(
        "{{var _b='{}';if(typeof updateWidget==='function'){{updateWidget(_b)}}}}",
        b64
    )
}

/// Generate an init script that pre-loads all widgets as data.
/// The page's IIFE reads `window.__WIDGET_BATCH` and calls `injectWidget` for each.
/// This avoids DOMContentLoaded timing issues with large payloads.
pub fn make_batch_init_js(widgets: &[(String, String, String)]) -> String {
    if widgets.is_empty() {
        return String::new();
    }
    let mut items = Vec::with_capacity(widgets.len());
    for (html, title, file) in widgets {
        let b64 = base64_encode(html.as_bytes());
        let t = title.replace('\\', "\\\\").replace('\'', "\\'");
        let f = file.replace('\\', "\\\\").replace('\'', "\\'");
        items.push(format!("{{b:'{}',t:'{}',f:'{}'}}", b64, t, f));
    }
    format!("window.__WIDGET_BATCH=[{}];", items.join(","))
}

/// Scan a directory for .html widget files, sorted by modification time (oldest first).
pub fn scan_widgets_dir(dir: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let mut entries: Vec<(std::path::PathBuf, String, Option<std::time::SystemTime>)> = Vec::new();
    if let Ok(dir_entries) = std::fs::read_dir(dir) {
        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "html") {
                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Widget".to_string());
                let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
                entries.push((path, title, mtime));
            }
        }
    }
    entries.sort_by(|a, b| a.2.cmp(&b.2));
    entries.into_iter().map(|(p, t, _)| (p, t)).collect()
}

/// Auto-discover the widgets directory. Checks CWD/.claude/widgets/ then ~/.claude/widgets/.
pub fn find_widgets_dir() -> Option<std::path::PathBuf> {
    // 1. Current working directory's project widgets
    if let Ok(cwd) = std::env::current_dir() {
        let dir = cwd.join(".claude").join("widgets");
        if dir.is_dir() {
            return Some(dir);
        }
    }
    // 2. User home widgets
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    find_widgets_in_home(std::path::Path::new(&home))
}

/// Check for widgets directory under a given home path.
/// Extracted from `find_widgets_dir` for testability — tests can call this
/// directly with a temp dir instead of mutating global env vars.
pub(crate) fn find_widgets_in_home(home: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = home.join(".claude").join("widgets");
    if dir.is_dir() { Some(dir) } else { None }
}

/// Check if a path points to a widget HTML file inside `.claude/widgets/`.
pub fn is_widget_path(path: &str) -> bool {
    (path.contains(".claude/widgets/") || path.contains(".claude\\widgets\\"))
        && path.ends_with(".html")
}

/// Base64 encoder — small, no dependencies, correctness verified by test.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4);

    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARS[((triple >> 18) & 0x3F) as usize]);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize]);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize]);
        } else {
            out.push(b'=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize]);
        } else {
            out.push(b'=');
        }
    }

    // SAFETY: output is only ASCII base64 characters
    unsafe { String::from_utf8_unchecked(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Base64 tests (existing) ---

    #[test]
    fn test_base64_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_one_byte() {
        assert_eq!(base64_encode(b"f"), "Zg==");
    }

    #[test]
    fn test_base64_two_bytes() {
        assert_eq!(base64_encode(b"fo"), "Zm8=");
    }

    #[test]
    fn test_base64_three_bytes() {
        assert_eq!(base64_encode(b"foo"), "Zm9v");
    }

    #[test]
    fn test_base64_padding_cases() {
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn test_base64_html_fragment() {
        let html = "<div>Hello <b>World</b></div>";
        let encoded = base64_encode(html.as_bytes());
        assert_eq!(encoded, "PGRpdj5IZWxsbyA8Yj5Xb3JsZDwvYj48L2Rpdj4=");
    }

    #[test]
    fn test_base64_non_ascii() {
        let input = "你好世界";
        let encoded = base64_encode(input.as_bytes());
        assert!(!encoded.is_empty());
        assert!(encoded.len() % 4 == 0);
    }

    // --- Scenario 3: batch widget loading ---

    #[test]
    fn test_scan_widgets_dir_finds_all_html() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.html", "b.html", "c.html", "d.html"] {
            std::fs::write(dir.path().join(name), "<p>hi</p>").unwrap();
        }
        std::fs::write(dir.path().join("data.txt"), "not a widget").unwrap();

        let result = scan_widgets_dir(dir.path());
        assert_eq!(result.len(), 4, "Should find exactly 4 .html files");
        for (path, title) in &result {
            assert!(path.extension().unwrap() == "html");
            // Title is derived from file stem
            assert!(!title.is_empty());
        }
    }

    #[test]
    fn test_scan_widgets_dir_mtime_ordering() {
        let dir = tempfile::tempdir().unwrap();

        // Create files with staggered mtimes (oldest → newest: old, mid, new)
        let old = dir.path().join("old.html");
        let mid = dir.path().join("mid.html");
        let new = dir.path().join("new.html");

        let now = std::time::SystemTime::now();
        let past = now - std::time::Duration::from_secs(200);
        let mid_time = now - std::time::Duration::from_secs(100);

        // Create all files, then set their mtimes
        std::fs::write(&old, "<p>old</p>").unwrap();
        std::fs::write(&mid, "<p>mid</p>").unwrap();
        std::fs::write(&new, "<p>new</p>").unwrap();

        // Use std::fs::FileTimes (stable since 1.75) to set modification times
        use std::fs::FileTimes;
        let f = std::fs::File::options().write(true).open(&old).unwrap();
        f.set_times(FileTimes::new().set_modified(past)).unwrap();
        let f = std::fs::File::options().write(true).open(&mid).unwrap();
        f.set_times(FileTimes::new().set_modified(mid_time)).unwrap();
        // `new` keeps its creation mtime (≈ now)

        let result = scan_widgets_dir(dir.path());
        assert_eq!(result.len(), 3);
        // Sorted oldest first
        assert_eq!(result[0].1, "old");
        assert_eq!(result[1].1, "mid");
        assert_eq!(result[2].1, "new");
    }

    #[test]
    fn test_scan_widgets_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let result = scan_widgets_dir(dir.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_make_batch_init_js_contains_all_widgets() {
        let widgets = vec![
            ("html1".to_string(), "Title 1".to_string(), "file1.html".to_string()),
            ("html2".to_string(), "Title 2".to_string(), "file2.html".to_string()),
            ("html3".to_string(), "Title 3".to_string(), "file3.html".to_string()),
        ];
        let js = make_batch_init_js(&widgets);
        assert!(js.starts_with("window.__WIDGET_BATCH=["), "Should start with batch assignment");
        assert!(js.ends_with("];"), "Should end with ];");
        let count = js.matches("{b:'").count();
        assert_eq!(count, 3, "Should contain 3 widget entries");
    }

    #[test]
    fn test_make_batch_init_js_empty() {
        let js = make_batch_init_js(&[]);
        assert!(js.is_empty(), "Empty input should produce empty string");
    }

    // --- Scenario 6: hook path matching ---

    #[test]
    fn test_is_widget_path_unix() {
        assert!(is_widget_path("/home/user/.claude/widgets/chart.html"));
    }

    #[test]
    fn test_is_widget_path_windows() {
        assert!(is_widget_path(r"C:\Users\user\.claude\widgets\chart.html"));
    }

    #[test]
    fn test_is_widget_path_rejects_non_widget() {
        assert!(!is_widget_path("src/main.rs"));
        assert!(!is_widget_path("/other/path/chart.html"));
    }

    #[test]
    fn test_is_widget_path_rejects_non_html() {
        assert!(!is_widget_path("/home/user/.claude/widgets/data.json"));
    }

    #[test]
    fn test_is_widget_path_edge_cases() {
        assert!(!is_widget_path(""));
        assert!(!is_widget_path("/home/user/.claude/widgets/chart")); // no extension
        // Subdirectory inside widgets — still a widget
        assert!(is_widget_path("/home/user/.claude/widgets/subdir/chart.html"));
        // Known limitation: no leading separator check before ".claude".
        // These match because contains() doesn't require a path separator before ".claude".
        // Acceptable for a hook filter — false positives are harmless (the file won't exist).
        assert!(is_widget_path("evil.claude/widgets/x.html"));
    }

    // --- Scenario 7: widget directory discovery ---

    #[test]
    fn test_find_widgets_in_home() {
        let tmp = tempfile::tempdir().unwrap();
        let widgets_dir = tmp.path().join(".claude").join("widgets");
        std::fs::create_dir_all(&widgets_dir).unwrap();

        // Directly test the home fallback logic — no env var mutation needed
        let result = find_widgets_in_home(tmp.path());
        assert_eq!(result, Some(widgets_dir));
    }

    #[test]
    fn test_find_widgets_in_home_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // No .claude/widgets/ created
        assert_eq!(find_widgets_in_home(tmp.path()), None);
    }
}
