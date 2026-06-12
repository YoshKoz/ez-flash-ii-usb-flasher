# PowerShell script to reset USB port for EZ-Writer
# Find the device
$device = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if (-not $device) {
    Write-Host "EZ-Writer device not found in PnP tree"
    exit 1
}
Write-Host "Found device: $($device.FriendlyName)"
Write-Host "InstanceId: $($device.InstanceId)"
Write-Host "Status: $($device.Status)"

# Disable and enable the device
$device | Disable-PnpDevice -Confirm:$false
Start-Sleep -Seconds 2
$device | Enable-PnpDevice -Confirm:$false
Start-Sleep -Seconds 3

# Check status
$device2 = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if ($device2) {
    Write-Host "After reset - Status: $($device2.Status)"
} else {
    Write-Host "After reset - Device not found in PnP tree"
}
