$exe = "$PSScriptRoot\ezwriter-cli.exe"
$fw = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\an2131_fw_v2.bin"
$t1 = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\loader_table1.bin"
$t2 = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\loader_table2.bin"
Write-Host "Step 1: waiting for bootloader (0547:2131) -- replug device now"
& $exe firmware-download $fw --watch
Write-Host "Step 2: init-exact (applying loader patches)"
Start-Sleep -Seconds 1
& $exe init-exact $t1 $t2
