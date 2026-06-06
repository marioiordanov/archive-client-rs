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
