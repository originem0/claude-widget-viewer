# Widget Protocol for Claude Code

When a request is better served by a visual/interactive widget than plain text, write the widget HTML to `.claude/widgets/<snake_case_name>.html`. The viewer renders it automatically.

## When to use widgets

Use widgets for: charts, diagrams, interactive explanations, data visualizations, UI mockups, mathematical visualizations. Keep text responses for everything else. When iterating on the same topic, overwrite the same file (hot-reload works).

## HTML format

Write a raw HTML fragment. No `<!DOCTYPE>`, `<html>`, `<head>`, or `<body>` tags. Structure: `<style>` first, content HTML second, `<script>` last.

```html
<style>
  .chart-container { padding: var(--spacing-md); }
</style>
<div class="chart-container">
  <canvas id="myChart"></canvas>
</div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() {
  // chart code here
}
if (window.Chart) initChart();
</script>
```

## Design rules

- Flat design: no gradients, mesh backgrounds, noise textures, or decorative effects
- Borders: 0.5px, generous whitespace, no shadows (except focus rings)
- Typography: only font-weight 400 and 500. h1=22px, h2=18px, h3=16px, body=16px, line-height 1.7
- Always sentence case, never Title Case or ALL CAPS
- Use CSS variables for all colors (never hardcode). Test: would every element be readable on a dark background?
- Category colors: purple, teal, coral, pink. Semantic colors reserved: blue=info, green=success, amber=warning, red=error
- Max 2-3 color ramps per widget
- SVG: viewBox width 680, default corner radius rx="4"
- No `position: fixed`, no tabs/carousels/`display:none` during render
- All content stacked vertically, container auto-sizes to content height

## CSS variables available

Text: `--color-text-primary`, `--color-text-secondary`
Background: `--color-bg-primary`, `--color-bg-secondary`
Border: `--color-border`, `--color-border-light`
Semantic: `--color-blue`, `--color-green`, `--color-amber`, `--color-red`
Category: `--color-purple`, `--color-teal`, `--color-coral`, `--color-pink`, `--color-gray`
Spacing: `--spacing-xs` (4px), `--spacing-sm` (8px), `--spacing-md` (16px), `--spacing-lg` (24px), `--spacing-xl` (32px)
Radius: `--border-radius-sm` (4px), `--border-radius-md` (8px), `--border-radius-lg` (12px)
Fonts: `--font-sans`, `--font-mono`

## SVG text classes

- `.t` — sans 14px, primary color
- `.ts` — sans 12px, secondary color
- `.th` — sans 14px, medium weight (500)
- `.c-blue`, `.c-teal`, `.c-purple`, `.c-coral`, `.c-pink`, `.c-amber`, `.c-green`, `.c-red`, `.c-gray` — fill + stroke

## CDN

Only `https://cdnjs.cloudflare.com` is allowed. Always use `onload` callback + fallback pattern for CDN scripts.

## Interaction stub

`window.sendPrompt(text)` is available but currently logs to console only. Do not build flows that depend on it.
