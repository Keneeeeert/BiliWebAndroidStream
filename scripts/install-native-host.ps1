$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent (Split-Path -Parent $scriptDir)
$source = if ($env:BILI_HELPER_BINARY) { $env:BILI_HELPER_BINARY } else { Join-Path $scriptDir "bili-web-android-stream-helper.exe" }
if (!(Test-Path $source)) { $source = Join-Path $root "bin\bili-web-android-stream-helper.exe" }
$install = Join-Path $env:LOCALAPPDATA "BiliWebAndroidStream"
$manifest = Join-Path $install "com.biliwebandroidstream.helper.json"
if (!(Test-Path $source)) { throw "Helper not found: $source" }
New-Item -ItemType Directory -Force $install | Out-Null
Copy-Item $source (Join-Path $install "bili-web-android-stream-helper.exe") -Force
@{ name="com.biliwebandroidstream.helper"; description="BiliWebAndroidStream native helper"; path=(Join-Path $install "bili-web-android-stream-helper.exe"); type="stdio"; allowed_extensions=@("biliwebandroidstream@example.invalid") } | ConvertTo-Json | Set-Content $manifest -Encoding UTF8
New-Item -Path "HKCU:\Software\Mozilla\NativeMessagingHosts\com.biliwebandroidstream.helper" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Mozilla\NativeMessagingHosts\com.biliwebandroidstream.helper" -Name '(default)' -Value $manifest
Write-Host "Installed helper and Firefox native host registration"
