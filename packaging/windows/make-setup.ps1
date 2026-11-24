# Builds the one-file Windows setup with Inno Setup 6 (free; preinstalled on
# GitHub's Windows runners, or `winget install JRSoftware.InnoSetup`). Run
# after `cargo build --release -p creabim-app -p creabim-cli`.
#   powershell -File packaging\windows\make-setup.ps1 -Out C:\path\to\folder
# The setup itself is CreaBIM.iss; it runs no PowerShell when installing or
# uninstalling (issue #53: the earlier IExpress setup did, hidden, and was
# flagged by an antivirus heuristic).
param(
    [string]$Out = ".",
    [string]$Target = "target\release",
    # the release day, so every file of a release carries the same date
    [string]$Date = ""
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$date = if ($Date) { $Date } else { Get-Date -Format "yyyy-MM-dd" }

# the files the setup installs, under the names it installs them as
$stage = Join-Path $env:TEMP "creabim-setup-stage"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
$bin = if ([IO.Path]::IsPathRooted($Target)) { $Target } else { Join-Path $root $Target }
Copy-Item (Join-Path $bin "creabim-app.exe") (Join-Path $stage "CreaBIM.exe")
Copy-Item (Join-Path $bin "creabim.exe") (Join-Path $stage "creabim-cli.exe")
Copy-Item (Join-Path $root "LICENSE"), (Join-Path $root "README.md") $stage
Copy-Item (Join-Path $root "docs\RELEASE_README.txt") (Join-Path $stage "READ ME FIRST.txt")
Copy-Item (Join-Path $root "crates\creabim-app\assets\logo\creabim.ico") $stage
Set-Content (Join-Path $stage "VERSION.txt") $version -Encoding ascii

# Inno Setup's compiler: on PATH, else where its installers put it
$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw "Inno Setup 6 not found: winget install JRSoftware.InnoSetup" }

New-Item -ItemType Directory -Force $Out | Out-Null
$outDir = (Resolve-Path $Out).Path
& $iscc /Q "/DVersion=$version" "/DReleaseDate=$date" "/DStage=$stage" "/DOut=$outDir" (Join-Path $PSScriptRoot "CreaBIM.iss")
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed ($LASTEXITCODE)" }
Get-Item (Join-Path $outDir "CreaBIM-$version-$date-windows-x64-setup.exe") | Select-Object Name, Length
