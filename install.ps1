#Requires -Version 5.1
<#
.SYNOPSIS
    One-click installer for claude-widget-viewer.
.DESCRIPTION
    Downloads the binary from GitHub Releases, deploys hook scripts,
    configures Claude Code settings.json, and installs the skill.
.PARAMETER Uninstall
    Remove claude-widget-viewer and undo all configuration changes.
#>
param(
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$repo = "originem0/claude-widget-viewer"
$binaryName = "claude-widget-viewer.exe"
$claudeDir = Join-Path $env:USERPROFILE ".claude"
$hooksDir = Join-Path $claudeDir "hooks"
$skillsDir = Join-Path $claudeDir "skills" "widget-viewer"
$settingsFile = Join-Path $claudeDir "settings.json"

# --- Detect install location ---
function Get-InstallDir {
    # Prefer scoop shims if scoop is installed
    $scoopShims = Join-Path $env:USERPROFILE "scoop" "shims"
    if (Test-Path $scoopShims) { return $scoopShims }

    # Fallback: ~/.local/bin (create if needed)
    $localBin = Join-Path $env:USERPROFILE ".local" "bin"
    if (-not (Test-Path $localBin)) { New-Item -ItemType Directory -Path $localBin -Force | Out-Null }
    # Ensure it's on PATH for this session
    if ($env:PATH -notlike "*$localBin*") {
        $env:PATH = "$localBin;$env:PATH"
        Write-Host "  Added $localBin to PATH for this session."
        Write-Host "  To make permanent, add to your system PATH."
    }
    return $localBin
}

# --- Uninstall ---
if ($Uninstall) {
    Write-Host "`n=== Uninstalling claude-widget-viewer ===" -ForegroundColor Yellow

    # Kill daemon
    Get-Process -Name "claude-widget-viewer" -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Host "  Stopped running processes."

    # Remove binary
    $installDir = Get-InstallDir
    $binaryPath = Join-Path $installDir $binaryName
    if (Test-Path $binaryPath) { Remove-Item $binaryPath -Force; Write-Host "  Removed $binaryPath" }

    # Remove hooks
    $hookFiles = @("widget-daemon-start.sh", "post-write-widget.sh")
    foreach ($f in $hookFiles) {
        $p = Join-Path $hooksDir $f
        if (Test-Path $p) { Remove-Item $p -Force; Write-Host "  Removed $p" }
    }

    # Remove skill
    if (Test-Path $skillsDir) { Remove-Item $skillsDir -Recurse -Force; Write-Host "  Removed skill." }

    # Remove hooks from settings.json
    if (Test-Path $settingsFile) {
        $settings = Get-Content $settingsFile -Raw | ConvertFrom-Json
        if ($settings.hooks) {
            $hooks = $settings.hooks
            $hooks.PSObject.Properties | Where-Object { $_.Name -in @("SessionStart", "PostToolUse") } | ForEach-Object {
                $hookArray = $_.Value
                $filtered = @($hookArray | Where-Object {
                    $inner = $_.hooks | Where-Object { $_.command -like "*widget*" }
                    -not $inner
                })
                if ($filtered.Count -eq 0) {
                    $hooks.PSObject.Properties.Remove($_.Name)
                } else {
                    $hooks.($_.Name) = $filtered
                }
            }
            $settings | ConvertTo-Json -Depth 10 | Set-Content $settingsFile -Encoding UTF8
            Write-Host "  Cleaned settings.json hooks."
        }
    }

    # Remove pipe id file
    $pipeFile = Join-Path $claudeDir ".widget-viewer-pipe"
    if (Test-Path $pipeFile) { Remove-Item $pipeFile -Force }

    Write-Host "`nUninstall complete.`n" -ForegroundColor Green
    exit 0
}

# --- Install ---
Write-Host "`n=== Installing claude-widget-viewer ===`n" -ForegroundColor Cyan

# Step 1: Check WebView2
Write-Host "[1/5] Checking WebView2 runtime..." -ForegroundColor White
$wv2Key = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
$wv2Installed = (Test-Path $wv2Key) -or (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue)
if ($wv2Installed) {
    Write-Host "  WebView2 runtime found." -ForegroundColor Green
} else {
    Write-Host "  WebView2 runtime NOT found." -ForegroundColor Red
    Write-Host "  Install from: https://developer.microsoft.com/en-us/microsoft-edge/webview2/" -ForegroundColor Yellow
    Write-Host "  Or run: winget install Microsoft.EdgeWebView2Runtime" -ForegroundColor Yellow
    exit 1
}

# Step 2: Download binary
Write-Host "[2/5] Downloading binary..." -ForegroundColor White
$installDir = Get-InstallDir
$binaryPath = Join-Path $installDir $binaryName

$apiUrl = "https://api.github.com/repos/$repo/releases/latest"
try {
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "claude-widget-viewer-installer" }
    $asset = $release.assets | Where-Object { $_.name -eq $binaryName } | Select-Object -First 1
    if (-not $asset) { throw "No binary asset found in latest release" }
    $downloadUrl = $asset.browser_download_url
    Write-Host "  Version: $($release.tag_name)"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $binaryPath -UseBasicParsing
    Write-Host "  Saved to $binaryPath" -ForegroundColor Green
} catch {
    Write-Host "  Failed to download from Releases: $_" -ForegroundColor Red
    Write-Host "  You can build from source: git clone + cargo build --release" -ForegroundColor Yellow
    exit 1
}

# Step 3: Deploy hook scripts
Write-Host "[3/5] Deploying hook scripts..." -ForegroundColor White
if (-not (Test-Path $hooksDir)) { New-Item -ItemType Directory -Path $hooksDir -Force | Out-Null }

$writeHook = @'
#!/bin/bash
FILE_PATH=$(jq -r '.tool_input.file_path // empty')
if [[ "$FILE_PATH" == *".claude/widgets/"*".html" ]] || [[ "$FILE_PATH" == *'.claude\widgets\'*'.html' ]]; then
  WIN_PATH=$(cygpath -w "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")
  cmd.exe /c "start /b claude-widget-viewer.exe send \"$WIN_PATH\"" </dev/null >/dev/null 2>&1
fi
exit 0
'@

Set-Content -Path (Join-Path $hooksDir "post-write-widget.sh") -Value $writeHook -NoNewline
Write-Host "  Hook deployed to $hooksDir" -ForegroundColor Green

# Check jq
if (-not (Get-Command jq -ErrorAction SilentlyContinue)) {
    Write-Host "  WARNING: jq not found. Hook scripts need jq to parse JSON." -ForegroundColor Yellow
    Write-Host "  Install: scoop install jq  OR  winget install jqlang.jq" -ForegroundColor Yellow
}

# Step 4: Configure settings.json
Write-Host "[4/5] Configuring Claude Code settings..." -ForegroundColor White
if (-not (Test-Path $claudeDir)) { New-Item -ItemType Directory -Path $claudeDir -Force | Out-Null }

$postToolUseHook = @{ matcher = "Write"; hooks = @(@{ type = "command"; command = "bash ~/.claude/hooks/post-write-widget.sh" }) }

if (Test-Path $settingsFile) {
    $settings = Get-Content $settingsFile -Raw | ConvertFrom-Json

    # Initialize hooks object if missing
    if (-not $settings.hooks) {
        $settings | Add-Member -NotePropertyName "hooks" -NotePropertyValue ([PSCustomObject]@{})
    }

    # Add PostToolUse hook (avoid duplicates)
    $existingPost = @()
    if ($settings.hooks.PSObject.Properties["PostToolUse"]) {
        $existingPost = @($settings.hooks.PostToolUse | Where-Object {
            -not ($_.hooks | Where-Object { $_.command -like "*post-write-widget*" })
        })
    }
    $settings.hooks | Add-Member -NotePropertyName "PostToolUse" -NotePropertyValue ($existingPost + $postToolUseHook) -Force

    $settings | ConvertTo-Json -Depth 10 | Set-Content $settingsFile -Encoding UTF8
} else {
    @{ hooks = @{ PostToolUse = @($postToolUseHook) } } |
        ConvertTo-Json -Depth 10 | Set-Content $settingsFile -Encoding UTF8
}
Write-Host "  settings.json updated." -ForegroundColor Green

# Step 5: Install skill
Write-Host "[5/5] Installing skill..." -ForegroundColor White
if (-not (Test-Path $skillsDir)) { New-Item -ItemType Directory -Path $skillsDir -Force | Out-Null }

$skillUrl = "https://raw.githubusercontent.com/$repo/main/claude/SKILL.md"
try {
    Invoke-WebRequest -Uri $skillUrl -OutFile (Join-Path $skillsDir "SKILL.md") -UseBasicParsing
    Write-Host "  Skill installed to $skillsDir" -ForegroundColor Green
} catch {
    Write-Host "  WARNING: Could not download skill. Install manually from claude/SKILL.md" -ForegroundColor Yellow
}

# Done
Write-Host "`n=== Installation complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "  Start a new Claude Code session and ask for a visualization."
Write-Host "  Or test manually: claude-widget-viewer show <file.html>"
Write-Host ""
Write-Host "  To uninstall: powershell -File install.ps1 -Uninstall"
Write-Host ""
