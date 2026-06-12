# Use WMI to disable/enable the USB device
$dev = Get-WmiObject -Class Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like "*0548*1005*" }
if (-not $dev) {
    Write-Host "EZ-Writer not found via WMI"
    exit 1
}
Write-Host "Found: $($dev.Name) PNPDeviceID=$($dev.PNPDeviceID)"
Write-Host "Status: $($dev.Status)"

Write-Host "Disabling..."
$result = $dev.Disable()
Write-Host "Disable result: $($result.ReturnValue)"
Start-Sleep -Seconds 3

Write-Host "Enabling..."
$result = $dev.Enable()
Write-Host "Enable result: $($result.ReturnValue)"
Start-Sleep -Seconds 3

$dev2 = Get-WmiObject -Class Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like "*0548*1005*" -or $_.PNPDeviceID -like "*0547*2131*" }
if ($dev2) {
    Write-Host "After cycle: $($dev2.Name) Status=$($dev2.Status)"
} else {
    Write-Host "After cycle: EZ-Writer not found"
}

# Also check libusb
Write-Host "`nlibusb check:"
python -c "import os; os.add_dll_directory(os.path.expanduser('~')); import usb.core; d = usb.core.find(idVendor=0x0548, idProduct=0x1005); print('active:', d); d2 = usb.core.find(idVendor=0x0547, idProduct=0x2131); print('bootloader:', d2)"
