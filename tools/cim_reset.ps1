# Broader search for EZ-Writer
$devices = Get-CimInstance -ClassName Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like "*0548*" -or $_.PNPDeviceID -like "*0547*" }
if (-not $devices) {
    Write-Host "EZ-Writer not found in CIM"
    
    # Try all USB devices
    $all = Get-CimInstance -ClassName Win32_PnPEntity | Where-Object { $_.PNPClass -eq "USB" } | Select-Object Name, PNPDeviceID, Status
    Write-Host "USB devices found:"
    $all | Format-Table -AutoSize | Out-String | Write-Host
    exit 1
}

foreach ($dev in $devices) {
    Write-Host "Found: $($dev.Name) PNPDeviceID=$($dev.PNPDeviceID) Status=$($dev.Status)"
    Write-Host "Disabling..."
    $result = Invoke-CimMethod -InputObject $dev -MethodName Disable
    Write-Host "Result: $($result.ReturnValue)"
    Start-Sleep -Seconds 3
    
    Write-Host "Enabling..."
    $result = Invoke-CimMethod -InputObject $dev -MethodName Enable
    Write-Host "Result: $($result.ReturnValue)"
    Start-Sleep -Seconds 5
    
    $dev2 = Get-CimInstance -ClassName Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like "*0548*" -or $_.PNPDeviceID -like "*0547*" }
    if ($dev2) {
        Write-Host "After cycle: $($dev2.Name) Status=$($dev2.Status)"
    } else {
        Write-Host "Device disappeared after cycle"
    }
}

Write-Host "`nlibusb check:"
python -c "import os; os.add_dll_directory(os.path.expanduser('~')); import usb.core; d = usb.core.find(idVendor=0x0548, idProduct=0x1005); print('active:', d); d2 = usb.core.find(idVendor=0x0547, idProduct=0x2131); print('bootloader:', d2)"
