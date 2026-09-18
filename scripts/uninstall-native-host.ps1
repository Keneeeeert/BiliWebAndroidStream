$ErrorActionPreference = "Stop"
Remove-Item "HKCU:\Software\Mozilla\NativeMessagingHosts\com.biliwebandroidstream.helper" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:LOCALAPPDATA "BiliWebAndroidStream") -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "Removed BiliWebAndroidStream helper and Firefox native host registration"
