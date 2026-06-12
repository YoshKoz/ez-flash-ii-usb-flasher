Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" } | Select-Object Status, FriendlyName, InstanceId | Format-List
Get-PnpDevice | Where-Object { $_.InstanceId -like "*0548*" } | Select-Object Status, FriendlyName, InstanceId | Format-List
# Also check if the hardware IDs exist via WMI
Get-CimInstance -ClassName Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like "*0547*" -or $_.PNPDeviceID -like "*0548*" } | Select-Object Status, Name, PNPDeviceID | Format-List
