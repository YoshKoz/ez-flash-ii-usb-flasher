# Try to find the USB hub device and restart it
$hubs = Get-CimInstance -ClassName Win32_USBHub
foreach ($hub in $hubs) {
    Write-Host "Hub: $($hub.Caption) DeviceID=$($hub.DeviceID) Status=$($hub.Status)"
    if ($hub.DeviceID -like "*VID_05E3*") {
        Write-Host "`nFound EZ-Writer hub. Trying to restart..."
        
        # Try disabling the hub
        try {
            Invoke-CimMethod -InputObject $hub -MethodName Disable -ErrorAction Stop
            Write-Host "Hub disabled. Waiting 10s..."
            Start-Sleep -Seconds 10
            
            Invoke-CimMethod -InputObject $hub -MethodName Enable -ErrorAction Stop
            Write-Host "Hub enabled."
            Start-Sleep -Seconds 5
        } catch {
            Write-Host "Hub restart failed: $_"
        }
    }
}

Write-Host "`nChecking for EZ-Writer..."
python -c "import os; os.add_dll_directory(os.path.expanduser('~')); import usb.core; d = usb.core.find(idVendor=0x0548, idProduct=0x1005); print('active:', d); d2 = usb.core.find(idVendor=0x0547, idProduct=0x2131); print('bootloader:', d2)"
