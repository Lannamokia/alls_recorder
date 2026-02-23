param(
    [Parameter(Position=0)]
    [string[]]$Components = @("server", "web-ui", "cli-capture"),
    
    [switch]$Help
)

if ($Help) {
    Write-Host "Usage: .\build_all.ps1 [component names...]"
    Write-Host ""
    Write-Host "Available components:"
    Write-Host "  server      - Build Rust server"
    Write-Host "  web-ui      - Build Web frontend"
    Write-Host "  cli-capture - Build CLI capture tool"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\build_all.ps1                    # Build all components"
    Write-Host "  .\build_all.ps1 server             # Build server only"
    Write-Host "  .\build_all.ps1 server web-ui      # Build server and frontend"
    Write-Host "  .\build_all.ps1 -Help              # Show this help"
    exit 0
}

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Dist = Join-Path $Root "dist"

$ValidComponents = @("server", "web-ui", "cli-capture")
foreach ($comp in $Components) {
    if ($comp -notin $ValidComponents) {
        Write-Host "Error: Unknown component '$comp'" -ForegroundColor Red
        Write-Host "Available components: $($ValidComponents -join ', ')"
        exit 1
    }
}

if (-not (Test-Path $Dist)) {
    New-Item -ItemType Directory -Path $Dist | Out-Null
}

function Build-Server {
    Write-Host "`n=== Building Server ===" -ForegroundColor Cyan
    Push-Location (Join-Path $Root "server")
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "Server build failed" }
        $ServerOut = Join-Path $Dist "server"
        if (-not (Test-Path $ServerOut)) {
            New-Item -ItemType Directory -Path $ServerOut | Out-Null
        }
        Copy-Item (Join-Path $Root "server\target\release\server.exe") -Destination (Join-Path $ServerOut "server.exe") -Force
        Write-Host "Server build succeeded" -ForegroundColor Green
    } finally {
        Pop-Location
    }
}

function Build-WebUI {
    Write-Host "`n=== Building Web-UI ===" -ForegroundColor Cyan
    Push-Location (Join-Path $Root "web-ui")
    try {
        npm install
        if ($LASTEXITCODE -ne 0) { throw "Web-UI npm install failed" }
        npm run build
        if ($LASTEXITCODE -ne 0) { throw "Web-UI build failed" }
        $WebOut = Join-Path $Dist "web-ui"
        if (-not (Test-Path $WebOut)) {
            New-Item -ItemType Directory -Path $WebOut | Out-Null
        }
        Copy-Item (Join-Path $Root "web-ui\dist\*") -Destination $WebOut -Recurse -Force
        Write-Host "Web-UI build succeeded" -ForegroundColor Green
    } finally {
        Pop-Location
    }
}

function Build-CliCapture {
    Write-Host "`n=== Building CLI-Capture ===" -ForegroundColor Cyan
    Push-Location (Join-Path $Root "cli-capture")
    try {
        cmd /c "scripts\build_windows.bat"
        if ($LASTEXITCODE -ne 0) { throw "CLI-Capture build failed" }
        $CliOut = Join-Path $Dist "cli-capture"
        if (-not (Test-Path $CliOut)) {
            New-Item -ItemType Directory -Path $CliOut | Out-Null
        }
        Copy-Item (Join-Path $Root "cli-capture\dist\*") -Destination $CliOut -Recurse -Force
        Write-Host "CLI-Capture build succeeded" -ForegroundColor Green
    } finally {
        Pop-Location
    }
}

Write-Host "Building components: $($Components -join ', ')" -ForegroundColor Yellow

foreach ($component in $Components) {
    switch ($component) {
        "server" { Build-Server }
        "web-ui" { Build-WebUI }
        "cli-capture" { Build-CliCapture }
    }
}

Write-Host "`nBuild completed: $Dist" -ForegroundColor Green
