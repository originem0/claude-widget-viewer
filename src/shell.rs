/// HTML shell generation.
/// Produces the base HTML document with CSS variables, morphdom, and widget container.
/// Widget HTML is injected via evaluate_script("injectWidget(base64)") from Rust.

const DESIGN_SYSTEM_CSS: &str = include_str!("../assets/design-system.css");
const MORPHDOM_JS: &str = include_str!("../assets/morphdom.min.js");

/// Generate the JS call to inject widget HTML via base64 encoding.
/// This avoids string escaping issues with evaluate_script.
pub fn make_inject_js(html: &str) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Writer::new(&mut buf);
        encoder.write_all(html.as_bytes()).unwrap();
        encoder.finish().unwrap();
    }
    let b64 = String::from_utf8(buf).unwrap();
    // Wrap in a ready-check: initialization scripts run before page scripts,
    // so injectWidget may not exist yet. Wait for DOMContentLoaded.
    format!(
        "if(typeof injectWidget==='function'){{injectWidget('{}')}}else{{document.addEventListener('DOMContentLoaded',function(){{injectWidget('{}')}})}}",
        b64, b64
    )
}

/// Minimal base64 encoder (no external dependency)
struct Base64Writer<'a> {
    out: &'a mut Vec<u8>,
    buf: [u8; 3],
    len: usize,
}

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<'a> Base64Writer<'a> {
    fn new(out: &'a mut Vec<u8>) -> Self {
        Self { out, buf: [0; 3], len: 0 }
    }
    fn flush_buf(&mut self) {
        if self.len == 0 { return; }
        let b = &self.buf;
        self.out.push(B64_CHARS[(b[0] >> 2) as usize]);
        self.out.push(B64_CHARS[((b[0] & 0x03) << 4 | b[1] >> 4) as usize]);
        if self.len > 1 {
            self.out.push(B64_CHARS[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize]);
        } else {
            self.out.push(b'=');
        }
        if self.len > 2 {
            self.out.push(B64_CHARS[(b[2] & 0x3f) as usize]);
        } else {
            self.out.push(b'=');
        }
        self.buf = [0; 3];
        self.len = 0;
    }
    fn finish(mut self) -> Result<(), std::io::Error> {
        self.flush_buf();
        Ok(())
    }
}

impl<'a> std::io::Write for Base64Writer<'a> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        for &byte in data {
            self.buf[self.len] = byte;
            self.len += 1;
            if self.len == 3 {
                self.flush_buf();
            }
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

pub fn generate_shell() -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Claude Widget</title>
<style>
{design_system}

* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
    font-family: var(--font-sans);
    font-size: 16px;
    line-height: 1.7;
    font-weight: 400;
    color: var(--color-text-primary);
    background: var(--color-bg-primary);
    padding: 20px;
}}
#widget-root {{
    max-width: 680px;
    margin: 0 auto;
}}
#loading {{
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
    color: var(--color-text-secondary);
    font-size: 14px;
}}
</style>
<script>
{morphdom}
</script>
</head>
<body>
<div id="widget-root">
    <div id="loading">Waiting for widget...</div>
</div>
<script>
(function() {{
    let isFirstLoad = true;

    // Called from Rust via evaluate_script("injectWidget('base64html')")
    window.injectWidget = function(b64) {{
        const html = atob(b64);
        if (!html) return;

        const root = document.getElementById('widget-root');

        if (isFirstLoad) {{
            root.innerHTML = html;
            executeScripts(root);
            isFirstLoad = false;
        }} else {{
            const temp = document.createElement('div');
            temp.innerHTML = html;
            morphdom(root, temp, {{
                childrenOnly: true,
                onBeforeElUpdated: function(fromEl, toEl) {{
                    if (fromEl === document.activeElement) return false;
                    return true;
                }}
            }});
            executeScripts(root);
        }}

        document.title = 'Claude Widget';
    }};

    function executeScripts(container) {{
        const scripts = container.querySelectorAll('script');
        scripts.forEach(oldScript => {{
            const newScript = document.createElement('script');
            Array.from(oldScript.attributes).forEach(attr => {{
                newScript.setAttribute(attr.name, attr.value);
            }});
            if (oldScript.src) {{
                newScript.src = oldScript.src;
            }} else {{
                newScript.textContent = oldScript.textContent;
            }}
            oldScript.parentNode.replaceChild(newScript, oldScript);
        }});
    }}

    window.sendPrompt = function(text) {{
        console.log('[sendPrompt stub]', text);
        window.ipc.postMessage(JSON.stringify({{ type: 'sendPrompt', text: text }}));
    }};
}})();
</script>
</body>
</html>"#,
        design_system = DESIGN_SYSTEM_CSS,
        morphdom = MORPHDOM_JS,
    )
}
