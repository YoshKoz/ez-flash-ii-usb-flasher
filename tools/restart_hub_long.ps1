# Restart the EZ-Writer's parent hub with longer delay
$hub = Get-PnpDevice -InstanceId 'USB\VID_05E3&PID_0610\6&23491980&0&1'
Write-Host "EZ-Writer hub: $($hub.FriendlyName) Status=$($hub.Status)"

Write-Host "Disabling USB hub (will power-cycle all devices on it)..."
Disable-PnpDevice -InstanceId $hub.InstanceId -Confirm:$false -ErrorAction Stop
Write-Host "Hub disabled. Waiting 10 seconds for power drain..."
Start-Sleep -Seconds 10

Write-Host "Re-enabling hub..."
Enable-PnpDevice -InstanceId $hub.InstanceId -Confirm:$false -ErrorAction Stop
Write-Host "Hub enabled. Waiting for enumeration..."
Start-Sleep -Seconds 5

$ez = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if ($ez) {
    Write-Host "EZ-Writer status: $($ez.Status)"
} else {
    Write-Host "EZ-Writer not found"
}
