$exe = "$PSScriptRoot\ezwriter-cli.exe"
$t1 = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\loader_table1.bin"
$t2 = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\loader_table2.bin"
& $exe init-exact $t1 $t2
