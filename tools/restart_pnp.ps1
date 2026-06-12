# Try to restart using Restart-PnpDevice
$ez = Get-PnpDevice -InstanceId 'USB\VID_0548&PID_1005\6&23491980&0&2'
if (-not $ez) {
    Write-Host "EZ-Writer not found"
    exit 1
}
Write-Host "EZ-Writer: Status=$($ez.Status) Problem=$($ez.Problem)"

# Try restart
try {
    Restart-PnpDevice -InstanceId $ez.InstanceId -Confirm:$false -ErrorAction Stop
    Write-Host "Restart-PnpDevice succeeded"
} catch {
    Write-Host "Restart-PnpDevice failed: $_"
}

Start-Sleep -Seconds 5

$ez2 = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if ($ez2) {
    Write-Host "After restart: $($ez2.Status)"
} else {
    Write-Host "After restart: device gone"
    # try scanning
    pnputil /scan-devices 2>&1
    Start-Sleep -Seconds 3
    $ez3 = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
    if ($ez3) {
        Write-Host "After scan: $($ez3.Status)"
    } else {
        Write-Host "Device not found"
    }
}

Write-Host "`nlibusb:"
python -c "import os; os.add_dll_directory(os.path.expanduser('~')); import usb.core; d = usb.core.find(idVendor=0x0548, idProduct=0x1005); print('active:', d); d2 = usb.core.find(idVendor=0x0547, idProduct=0x2131); print('bootloader:', d2)"
