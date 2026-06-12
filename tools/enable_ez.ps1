$dev = Get-PnpDevice -InstanceId 'USB\VID_0548&PID_1005\6&23491980&0&2'
Write-Host "Current Status: $($dev.Status)"
$dev | Enable-PnpDevice -Confirm:$false
Start-Sleep -Seconds 3
$dev2 = Get-PnpDevice -InstanceId 'USB\VID_0548&PID_1005\6&23491980&0&2'
if ($dev2) {
    Write-Host "After enable - Status: $($dev2.Status)"
} else {
    Write-Host "Device disappeared"
    # Try to find any EZ device again
    $all = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
    if ($all) {
        $all | Select-Object Status, FriendlyName, InstanceId | Format-List
    } else {
        Write-Host "No EZ-Writer device found"
    }
}
