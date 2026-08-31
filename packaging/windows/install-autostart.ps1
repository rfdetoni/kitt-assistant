param([Parameter(Mandatory=$true)][string]$KittdPath)
$action=New-ScheduledTaskAction -Execute $KittdPath
$trigger=New-ScheduledTaskTrigger -AtLogOn
$settings=New-ScheduledTaskSettingsSet -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Days 3650)
Register-ScheduledTask -TaskName "KITT Assistant" -Action $action -Trigger $trigger -Settings $settings -Description "Starts the low-footprint KITT Assistant daemon" -Force
