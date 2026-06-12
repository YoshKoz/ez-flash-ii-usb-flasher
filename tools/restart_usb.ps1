# PowerShell: Force rescan all USB host controllers
$controllers = Get-PnpDevice | Where-Object { $_.Class -eq "USB" -and $_.FriendlyName -like "*Host Controller*" }
foreach ($c in $controllers) {
    Write-Host "Restarting: $($c.FriendlyName) ($($c.InstanceId))"
    $c | Disable-PnpDevice -Confirm:$false
    Start-Sleep -Seconds 2
    $c | Enable-PnpDevice -Confirm:$false
    Start-Sleep -Seconds 3
}
Write-Host "Done. Waiting for EZ-Writer..."
Start-Sleep -Seconds 5
$ez = Get-PnpDevice | Where-Object { $_.InstanceId -like "*0547*" -or $_.InstanceId -like "*0548*" }
if ($ez) {
    Write-Host "EZ-Writer found: $($ez.Status)"
} else {
    Write-Host "EZ-Writer not found after controller restart"
}
