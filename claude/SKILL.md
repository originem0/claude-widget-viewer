---
name: widget-viewer
description: >
  Use when the user asks for charts, diagrams, visualizations, interactive explanations,
  data plots, UI mockups, or any visual content better shown graphically than as text.
  Triggers: "画图", "图表", "可视化", "展示", "visualize", "chart", "diagram", "plot",
  "show me", "draw", "interactive". Renders widget HTML in a native WebView2 window
  via claude-widget-viewer.exe.
---

# Widget Viewer

Render interactive HTML widgets in a native WebView2 window. Write widget HTML to `.claude/widgets/<name>.html` — a hook auto-launches the viewer.

## Quick Reference

| Item | Value |
|------|-------|
| Output path | `.claude/widgets/<snake_case_name>.html` |
| Format | Raw fragment: `<style>` → content → `<script>` |
| CDN | Only `https://cdnjs.cloudflare.com` |
| Max colors | 2-3 ramps per widget |
| Container width | 680px |
| Manual test | `claude-widget-viewer show <file>` |

## HTML Format

Write a raw HTML fragment. No `<!DOCTYPE>`, `<html>`, `<head>`, `<body>`.

```html
<style>
  .card { padding: var(--spacing-md); background: var(--color-bg-secondary); border-radius: var(--border-radius-lg); }
</style>
<div class="card">
  <h2>Title</h2>
  <canvas id="myChart"></canvas>
</div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() { /* ... */ }
if (window.Chart) initChart();
</script>
```

Structure order is load-bearing: style first (avoids FOUC), content second, script last.

## CSS Variables

**Text:** `--color-text-primary`, `--color-text-secondary`
**Background:** `--color-bg-primary`, `--color-bg-secondary`
**Border:** `--color-border`, `--color-border-light`
**Semantic:** `--color-blue` (info), `--color-green` (success), `--color-amber` (warning), `--color-red` (error)
**Category:** `--color-purple`, `--color-teal`, `--color-coral`, `--color-pink`, `--color-gray`
**Spacing:** `--spacing-xs` (4), `--spacing-sm` (8), `--spacing-md` (16), `--spacing-lg` (24), `--spacing-xl` (32)
**Radius:** `--border-radius-sm` (4), `--border-radius-md` (8), `--border-radius-lg` (12)
**Fonts:** `--font-sans`, `--font-mono`

## Design Rules

- Flat: no gradients, mesh backgrounds, decorative effects
- Borders 0.5px, generous whitespace, no shadows (except focus rings)
- Font-weight 400 and 500 only. h1=22px, h2=18px, h3=16px, body=16px, line-height 1.7
- Sentence case always, never Title Case or ALL CAPS
- Never hardcode colors — always use CSS variables
- Category colors: purple, teal, coral, pink. Semantic reserved: blue/green/amber/red
- SVG: viewBox width 680, rx="4" default
- No `position: fixed`, no tabs/carousels/`display:none`
- All content vertical stack, container auto-sizes

## CDN Script Pattern

Always use `onload` + fallback — CDN may not load before inline scripts execute:

```html
<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() { /* chart code */ }
if (window.Chart) initChart();
</script>
```

## SVG Text Classes

`.t` (sans 14px primary), `.ts` (sans 12px secondary), `.th` (sans 14px medium)
Color: `.c-blue`, `.c-teal`, `.c-purple`, `.c-coral`, `.c-pink`, `.c-amber`, `.c-green`, `.c-red`, `.c-gray`

## When to Use vs Not

**Use:** data visualization, flow diagrams, interactive sliders/controls, chart comparisons, architecture diagrams, math visualizations, UI prototypes

**Don't use:** pure text answers, code explanations, simple lists, anything where text is clearer

When iterating on the same topic, overwrite the same filename — hot-reload updates the window without reopening.
