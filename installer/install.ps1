#Requires -Version 5.1
<#
.SYNOPSIS
    Installs dryve from GitHub Releases on Windows.
.EXAMPLE
    irm https://raw.githubusercontent.com/notenderdreams/dryve/main/installer/install.ps1 | iex
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Configuration ---

$Repo       = "notenderdreams/dryve"
$Binary     = "dryve"
$InstallDir = Join-Path $env:USERPROFILE ".dryve\bin"

# --- Helpers ---

function Write-Info  { param([string]$Message) Write-Host "[info]  $Message" -ForegroundColor Blue }
function Write-Err   { param([string]$Message) Write-Host "[error] $Message" -ForegroundColor Red; exit 1 }

# --- Detect architecture ---

function Get-PlatformArch {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLower()
    switch ($arch) {
        'x64'   { return 'x86_64' }
        'arm64' { return 'arm64'  }
        default { Write-Err "Unsupported architecture: $arch" }
    }
}

# --- Fetch latest tag ---

function Get-LatestVersion {
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $response = Invoke-RestMethod -Uri $url -UseBasicParsing
        return $response.tag_name
    }
    catch {
        Write-Err "Failed to fetch latest release from GitHub. $_"
    }
}

# --- Add to PATH (current user) ---

function Add-ToUserPath {
    param([string]$Dir)

    $currentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($currentPath -split ';' | Where-Object { $_ -eq $Dir }) {
        Write-Info "Directory already in user PATH."
        return
    }

    $newPath = "$currentPath;$Dir"
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$Dir"
    Write-Info "Added '$Dir' to user PATH."
}

# --- Main ---

function Install-App {
    $arch = Get-PlatformArch
    Write-Info "Detected platform: windows-$arch"

    Write-Info "Fetching latest release..."
    $Version = Get-LatestVersion

    if (-not $Version) {
        Write-Err "Could not determine latest version."
    }

    Write-Info "Version: $Version"

    $asset    = "$Binary-$Version-windows-$arch.zip"
    $url      = "https://github.com/$Repo/releases/download/$Version/$asset"
    $tmpDir   = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())

    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null
    $zipPath = Join-Path $tmpDir $asset

    Write-Info "Downloading $url..."
    try {
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
    }
    catch {
        Write-Err "Download failed. Check that the release exists. $_"
    }

    Write-Info "Extracting..."
    Expand-Archive -Path $zipPath -DestinationPath $tmpDir -Force

    # Ensure install directory exists
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Find the binary (could be at root or in a subfolder)
    $exeName = "$Binary.exe"
    $exePath = Get-ChildItem -Path $tmpDir -Filter $exeName -Recurse | Select-Object -First 1

    if (-not $exePath) {
        Write-Err "Could not find '$exeName' in the downloaded archive."
    }

    $destination = Join-Path $InstallDir $exeName
    Write-Info "Installing to $destination..."
    Copy-Item -Path $exePath.FullName -Destination $destination -Force

    # Clean up
    Remove-Item -Recurse -Force $tmpDir

    # Add to PATH
    Add-ToUserPath $InstallDir

    Write-Info "$Binary $Version installed to $destination"
    Write-Info "Restart your terminal, then run 'dryve --help' to get started."
}

Install-App