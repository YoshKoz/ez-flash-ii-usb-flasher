$exe = "$PSScriptRoot\ezwriter-cli.exe"
$fw = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\an2131_fw_v2.bin"
$lt = "C:\Development\ez-flash-ii-usb-flasher\src\ezwriter-cli\loader_table2.bin"
& $exe firmware-download $fw --loader-table $lt --force
