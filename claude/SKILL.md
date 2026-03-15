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

Write widget HTML to `.claude/widgets/<snake_case_name>.html` — a hook auto-launches a native WebView2 window. Raw fragment only: no `<!DOCTYPE>`, `<html>`, `<head>`, `<body>`. Structure: `<style>` → content → `<script>`.

## Mandatory Rules

- NEVER use fixed pixel widths on containers. All containers: `width: 100%`.
- ALWAYS read colors from CSS variables via `getComputedStyle`. Never hardcode hex values.
- ALWAYS set `responsive: true` on Chart.js. ALWAYS wrap `<canvas>` in `<div class="chart-wrap">`.
- ALWAYS use `onload` + fallback for CDN scripts.
- CDN whitelist: `https://cdnjs.cloudflare.com`, `https://cdn.jsdelivr.net`, `https://unpkg.com`. Fonts: `https://fonts.googleapis.com`.
- SVG: ALWAYS use `viewBox` + `width="100%"`. Never set fixed pixel width on `<svg>`.
- Overwrite same filename when iterating — hot-reload updates without reopening.

## Template: Chart.js (line / bar / doughnut)

Copy this template. Change `type`, `labels`, `datasets`, and title. Do NOT remove tooltip/animation/scale config.

```html
<style>
.widget-card {
  background: var(--color-bg-secondary);
  border: 0.5px solid var(--color-border);
  border-radius: var(--border-radius-lg);
  padding: var(--spacing-lg);
}
.widget-card h2 { font-size: 18px; font-weight: 500; margin-bottom: 2px; }
.widget-card .subtitle { color: var(--color-text-secondary); font-size: 14px; margin-bottom: var(--spacing-md); }
</style>

<div class="widget-card">
  <h2>Chart title here</h2>
  <p class="subtitle">Brief description of the data</p>
  <div class="chart-wrap"><canvas id="mainChart"></canvas></div>
</div>

<script src="https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js" onload="initChart()"></script>
<script>
function initChart() {
  var cs = getComputedStyle(document.documentElement);
  var c = function(v) { return cs.getPropertyValue(v).trim(); };
  // Safe semi-transparent color: parse hex → rgba
  function alpha(cssVar, a) {
    var hex = c(cssVar);
    var r = parseInt(hex.slice(1,3), 16), g = parseInt(hex.slice(3,5), 16), b = parseInt(hex.slice(5,7), 16);
    return 'rgba(' + r + ',' + g + ',' + b + ',' + a + ')';
  }

  new Chart(document.getElementById('mainChart'), {
    type: 'line',
    data: {
      labels: ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'],
      datasets: [{
        label: 'Series A',
        data: [12, 15, 13, 17, 20, 18, 16],
        borderColor: c('--color-teal'),
        backgroundColor: alpha('--color-teal', 0.1),
        fill: true,
        tension: 0.35,
        pointRadius: 4,
        pointHoverRadius: 6,
        pointBackgroundColor: c('--color-teal'),
        borderWidth: 2
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: true,
      animation: { duration: 600, easing: 'easeOutQuart' },
      interaction: { mode: 'index', intersect: false },
      plugins: {
        legend: {
          display: true,
          position: 'bottom',
          labels: { padding: 16, usePointStyle: true, font: { size: 13 } }
        },
        tooltip: {
          backgroundColor: c('--color-bg-secondary'),
          titleColor: c('--color-text-primary'),
          bodyColor: c('--color-text-secondary'),
          borderColor: c('--color-border'),
          borderWidth: 0.5,
          padding: 10,
          cornerRadius: 8,
          displayColors: true,
          boxPadding: 4
        }
      },
      scales: {
        y: {
          grid: { color: c('--color-border-light'), lineWidth: 0.5 },
          ticks: { color: c('--color-text-secondary'), font: { size: 12 } }
        },
        x: {
          grid: { display: false },
          ticks: { color: c('--color-text-secondary'), font: { size: 12 } }
        }
      }
    }
  });
}
if (window.Chart) initChart();
</script>
```

For bar charts: change `type: 'bar'`, remove `fill`/`tension`/`pointRadius`, add `borderRadius: 4` to dataset.
For doughnut: change `type: 'doughnut'`, remove `scales` entirely, use multiple colors: `backgroundColor: [c('--color-teal'), c('--color-purple'), c('--color-coral'), c('--color-blue')]`.
For multiple datasets: add objects to `datasets[]`, use different `--color-*` variables for each.

## Template: SVG Diagram

Copy this template for flow diagrams, architecture diagrams, or custom visualizations.

```html
<style>
.diagram-card {
  background: var(--color-bg-secondary);
  border: 0.5px solid var(--color-border);
  border-radius: var(--border-radius-lg);
  padding: var(--spacing-lg);
}
.diagram-card h2 { font-size: 18px; font-weight: 500; margin-bottom: var(--spacing-md); }
.node { transition: opacity 0.15s ease; }
.node:hover { opacity: 0.8; }
</style>

<div class="diagram-card">
  <h2>Diagram title here</h2>
  <div class="svg-wrap">
    <svg viewBox="0 0 800 400" width="100%" preserveAspectRatio="xMidYMid meet">
      <defs>
        <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-text-secondary)"/>
        </marker>
      </defs>

      <rect x="50" y="160" width="140" height="60" rx="4"
            fill="var(--color-bg-primary)" stroke="var(--color-border)" stroke-width="0.5" class="node"/>
      <text x="120" y="195" text-anchor="middle" class="t">Node A</text>

      <line x1="190" y1="190" x2="280" y2="190" stroke="var(--color-text-secondary)" stroke-width="1" marker-end="url(#arrow)"/>

      <rect x="280" y="160" width="140" height="60" rx="4"
            fill="var(--color-bg-primary)" stroke="var(--color-teal)" stroke-width="1" class="node"/>
      <text x="350" y="195" text-anchor="middle" class="t">Node B</text>
    </svg>
  </div>
</div>
```

SVG text classes (from design system): `.t` (14px primary), `.ts` (12px secondary), `.th` (14px medium weight)
SVG color classes: `.c-blue`, `.c-teal`, `.c-purple`, `.c-coral`, `.c-pink`, `.c-amber`, `.c-green`, `.c-red`, `.c-gray`

## Template: D3.js

For complex interactive visualizations. Use D3 v7 from CDN. Tooltip uses `position: fixed` to avoid clipping by overflow containers.

```html
<style>
.d3-card {
  background: var(--color-bg-secondary);
  border: 0.5px solid var(--color-border);
  border-radius: var(--border-radius-lg);
  padding: var(--spacing-lg);
}
.d3-card h2 { font-size: 18px; font-weight: 500; margin-bottom: var(--spacing-md); }
.d3-tooltip {
  position: fixed; visibility: hidden; pointer-events: none; z-index: 1000;
  background: var(--color-bg-secondary); border: 0.5px solid var(--color-border);
  padding: 8px 12px; border-radius: 8px; font-size: 13px;
  color: var(--color-text-primary);
}
</style>

<div class="d3-card">
  <h2>D3 chart title</h2>
  <div id="d3-container" style="width: 100%;"></div>
</div>
<div class="d3-tooltip" id="tooltip"></div>

<script src="https://cdnjs.cloudflare.com/ajax/libs/d3/7.9.0/d3.min.js" onload="initD3()"></script>
<script>
function initD3() {
  var cs = getComputedStyle(document.documentElement);
  var c = function(v) { return cs.getPropertyValue(v).trim(); };

  var container = document.getElementById('d3-container');
  var rect = container.getBoundingClientRect();
  var width = rect.width;
  var height = width * 0.5;
  var margin = { top: 20, right: 20, bottom: 40, left: 50 };
  var innerW = width - margin.left - margin.right;
  var innerH = height - margin.top - margin.bottom;

  var svg = d3.select('#d3-container').append('svg')
    .attr('width', '100%').attr('height', height)
    .attr('viewBox', '0 0 ' + width + ' ' + height);

  var g = svg.append('g').attr('transform', 'translate(' + margin.left + ',' + margin.top + ')');
  var tooltip = d3.select('#tooltip');

  var data = [
    { label: 'A', value: 30 }, { label: 'B', value: 80 },
    { label: 'C', value: 45 }, { label: 'D', value: 60 },
    { label: 'E', value: 20 }, { label: 'F', value: 90 }
  ];

  var x = d3.scaleBand().domain(data.map(function(d) { return d.label; }))
    .range([0, innerW]).padding(0.25);
  var y = d3.scaleLinear().domain([0, d3.max(data, function(d) { return d.value; }) * 1.1])
    .range([innerH, 0]);

  g.append('g').attr('transform', 'translate(0,' + innerH + ')').call(d3.axisBottom(x))
    .selectAll('text').style('fill', c('--color-text-secondary')).style('font-size', '12px');
  g.append('g').call(d3.axisLeft(y).ticks(5))
    .selectAll('text').style('fill', c('--color-text-secondary')).style('font-size', '12px');
  g.selectAll('.domain, .tick line').attr('stroke', c('--color-border-light'));

  g.selectAll('rect').data(data).join('rect')
    .attr('x', function(d) { return x(d.label); })
    .attr('width', x.bandwidth())
    .attr('y', innerH).attr('height', 0)
    .attr('fill', c('--color-teal')).attr('rx', 4)
    .on('mouseover', function(event, d) {
      d3.select(this).attr('opacity', 0.8);
      tooltip.style('visibility', 'visible').html('<b>' + d.label + '</b>: ' + d.value);
    })
    .on('mousemove', function(event) {
      tooltip.style('top', (event.clientY - 30) + 'px').style('left', (event.clientX + 10) + 'px');
    })
    .on('mouseout', function() {
      d3.select(this).attr('opacity', 1); tooltip.style('visibility', 'hidden');
    })
    .transition().duration(600).ease(d3.easeCubicOut)
    .attr('y', function(d) { return y(d.value); })
    .attr('height', function(d) { return innerH - y(d.value); });
}
if (window.d3) initD3();
</script>
```

## CSS Variables Available

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
- Category colors: purple, teal, coral, pink. Semantic reserved: blue=info, green=success, amber=warning, red=error
- Tooltips: use `position: fixed` + `z-index: 1000` to avoid clipping
- All content vertical stack, container auto-sizes to content height

## When to Use vs Not

**Use:** data visualization, flow diagrams, interactive sliders/controls, chart comparisons, architecture diagrams, math visualizations, UI prototypes

**Don't use:** pure text answers, code explanations, simple lists, anything where text is clearer
