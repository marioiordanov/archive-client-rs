$repo = "marioiordanov/archive-client-rs"
$asset = "archive-client-rs-windows-x86_64.zip"
$installDir = "$env:ProgramFiles\archive-client-rs"

$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$url = ($release.assets | Where-Object { $_.name -eq $asset }).browser_download_url

if (-not $url) {
    Write-Error "Asset '$asset' not found in latest release."
    exit 1
}

$zip = "$env:TEMP\$asset"
Invoke-WebRequest -Uri $url -OutFile $zip

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Expand-Archive -Path $zip -DestinationPath $installDir -Force
Remove-Item $zip

Write-Host "Installed to $installDir"

$DllPath = Join-Path -Path $installDir -ChildPath "ArchiveContextMenu.dll"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    $argList = "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -DllPath `"$DllPath`""
    if ($Unregister) { $argList += " -Unregister" }
    Start-Process powershell -Verb RunAs -ArgumentList $argList -Wait
    exit
}

if (-not (Test-Path $DllPath)) {
    Write-Error "DLL not found: $DllPath`nBuild first: cmake -B build -A x64 && cmake --build build --config Release"
    Read-Host "Press Enter to exit"
    exit 1
}

$DllPath = (Resolve-Path $DllPath).Path
Write-Host $DllPath

$proc = Start-Process regsvr32.exe -ArgumentList "/s `"$DllPath`"" -Wait -PassThru
if ($proc.ExitCode -eq 0) {
    Write-Host "Registered: $DllPath"
    Write-Host "Right-click any file to see 'Archived Versions'."
} else {
    Write-Error ("regsvr32 failed (exit 0x{0:X8})" -f $proc.ExitCode)
}
