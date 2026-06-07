param(
    [string]$DllPath = "$PSScriptRoot\build\Release\ArchiveContextMenu.dll",
    [switch]$Unregister
)

# Self-elevate: relaunch as Administrator if not already elevated
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    $argList = "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -DllPath `"$DllPath`""
    if ($Unregister) { $argList += " -Unregister" }
    Start-Process powershell -Verb RunAs -ArgumentList $argList -Wait
    exit
}

# Resolve absolute path before any registry operations
if (-not (Test-Path $DllPath)) {
    Write-Error "DLL not found: $DllPath`nBuild first: cmake -B build -A x64 && cmake --build build --config Release"
    Read-Host "Press Enter to exit"
    exit 1
}
$DllPath = (Resolve-Path $DllPath).Path

if ($Unregister) {
    $proc = Start-Process regsvr32.exe -ArgumentList "/u /s `"$DllPath`"" -Wait -PassThru
    if ($proc.ExitCode -eq 0) {
        Write-Host "Unregistered: $DllPath"
    } else {
        Write-Error ("regsvr32 /u failed (exit 0x{0:X8})" -f $proc.ExitCode)
    }
} else {
    $proc = Start-Process regsvr32.exe -ArgumentList "/s `"$DllPath`"" -Wait -PassThru
    if ($proc.ExitCode -eq 0) {
        Write-Host "Registered: $DllPath"
        Write-Host "Right-click any file to see 'Archived Versions'."
    } else {
        Write-Error ("regsvr32 failed (exit 0x{0:X8})" -f $proc.ExitCode)
    }
}

Read-Host "Press Enter to exit"
