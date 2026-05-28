$ErrorActionPreference = "Stop"

$Repo = "JohnKesko/bfiles"
$Asset = "bfiles-windows-x86_64.zip"
$BinName = "bfiles.exe"
$InstallDir = Join-Path $env:USERPROFILE ".local\bin"

$Url = "https://github.com/$Repo/releases/latest/download/$Asset"
$TempDir = Join-Path $env:TEMP ("bfiles-install-" + [System.Guid]::NewGuid().ToString())

New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
    $ZipPath = Join-Path $TempDir $Asset

    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $ZipPath

    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

    $Binary = Get-ChildItem -Path $TempDir -Recurse -Filter $BinName | Select-Object -First 1

    if (-not $Binary) {
        throw "Could not find $BinName in archive"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item $Binary.FullName (Join-Path $InstallDir $BinName) -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if (($UserPath -split ";") -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        Write-Host "Added $InstallDir to your user PATH."
        Write-Host "Restart your terminal before running it."
    }

    Write-Host "Installed bfiles to $InstallDir"
}
finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}