/// HTML shell generation.
/// Fix #6: base64 stored in variable, not duplicated in format string.
/// Fix #8: no fetch/protocol needed — widget injected via evaluate_script.

const DESIGN_SYSTEM_CSS: &str = include_str!("../assets/design-system.css");
const MORPHDOM_JS: &str = include_str!("../assets/morphdom.min.js");

/// Generate JS to inject widget HTML via base64 encoding.
/// Fix #6: uses a JS variable so base64 payload appears only once.
pub fn make_inject_js(html: &str) -> String {
    let b64 = base64_encode(html.as_bytes());
    format!(
        "{{var _b='{}';if(typeof injectWidget==='function'){{injectWidget(_b)}}else{{document.addEventListener('DOMContentLoaded',function(){{injectWidget(_b)}})}}}}",
        b64
    )
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
    var isFirstLoad = true;

    window.injectWidget = function(b64) {{
        // Decode base64 → binary → UTF-8 (atob alone mangles multibyte chars)
        var bin = atob(b64);
        var bytes = new Uint8Array(bin.length);
        for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        var html = new TextDecoder('utf-8').decode(bytes);
        if (!html) return;

        var root = document.getElementById('widget-root');

        if (isFirstLoad) {{
            root.innerHTML = html;
            executeScripts(root);
            isFirstLoad = false;
        }} else {{
            var temp = document.createElement('div');
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
        var scripts = container.querySelectorAll('script');
        scripts.forEach(function(oldScript) {{
            var newScript = document.createElement('script');
            Array.from(oldScript.attributes).forEach(function(attr) {{
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
        if (window.ipc) {{
            window.ipc.postMessage(JSON.stringify({{ type: 'sendPrompt', text: text }}));
        }}
    }};
}})();
</script>
</body>
</html>"#,
        design_system = DESIGN_SYSTEM_CSS,
        morphdom = MORPHDOM_JS,
    )
}

// Fix #5: base64 encoder has tests
#[cfg(test)]
mod tests {
    use super::base64_encode;

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
        // Verify roundtrip: JS atob(encoded) should produce original
        // We verify the known base64 encoding
        assert_eq!(encoded, "PGRpdj5IZWxsbyA8Yj5Xb3JsZDwvYj48L2Rpdj4=");
    }

    #[test]
    fn test_base64_non_ascii() {
        let input = "你好世界";
        let encoded = base64_encode(input.as_bytes());
        assert!(!encoded.is_empty());
        assert!(encoded.len() % 4 == 0);
    }
}
