# Find parent hub of EZ-Writer
$ez = Get-PnpDevice -InstanceId 'USB\VID_0548&PID_1005\6&23491980&0&2'
if (-not $ez) {
    Write-Host "EZ-Writer not found"
    exit 1
}

Write-Host "EZ-Writer: $($ez.FriendlyName) Status=$($ez.Status)"

# Get all USB hub devices
$hubs = Get-PnpDevice | Where-Object { $_.Class -eq "USB" -and $_.FriendlyName -like "*Hub*" }
foreach ($hub in $hubs) {
    Write-Host "Hub: $($hub.FriendlyName) InstanceId=$($hub.InstanceId) Status=$($hub.Status)"
}

# Try restarting a generic USB hub
$genericHubs = Get-PnpDevice | Where-Object { $_.Class -eq "USB" -and $_.FriendlyName -eq "USB Hub" -and $_.Status -eq "OK" }
foreach ($hub in $genericHubs) {
    Write-Host "Restarting hub: $($hub.InstanceId)"
    Disable-PnpDevice -InstanceId $hub.InstanceId -Confirm:$false -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    Enable-PnpDevice -InstanceId $hub.InstanceId -Confirm:$false -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
}

# Check if EZ-Writer status changed
$ez2 = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if ($ez2) {
    Write-Host "After hub restart: $($ez2.Status)"
} else {
    Write-Host "After hub restart: device disappeared"
}
