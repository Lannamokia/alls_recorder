$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Dist = Join-Path $Root "dist"

if (Test-Path $Dist) {
  Remove-Item $Dist -Recurse -Force
}

New-Item -ItemType Directory -Path $Dist | Out-Null

Push-Location (Join-Path $Root "server")
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$ServerOut = Join-Path $Dist "server"
New-Item -ItemType Directory -Path $ServerOut | Out-Null
Copy-Item (Join-Path $Root "server\target\release\server.exe") -Destination (Join-Path $ServerOut "server.exe") -Force
Pop-Location

Push-Location (Join-Path $Root "web-ui")
npm install
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$WebOut = Join-Path $Dist "web-ui"
New-Item -ItemType Directory -Path $WebOut | Out-Null
Copy-Item (Join-Path $Root "web-ui\dist\*") -Destination $WebOut -Recurse -Force
Pop-Location

Push-Location (Join-Path $Root "cli-capture")
cmd /c "scripts\build_windows.bat"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$CliOut = Join-Path $Dist "cli-capture"
New-Item -ItemType Directory -Path $CliOut | Out-Null
Copy-Item (Join-Path $Root "cli-capture\dist\*") -Destination $CliOut -Recurse -Force
Pop-Location

Write-Host "构建完成：$Dist"
