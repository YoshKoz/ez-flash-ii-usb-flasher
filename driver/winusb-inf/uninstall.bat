@echo off
setlocal EnableDelayedExpansion

echo ============================================================
echo EZ-Writer II WinUSB Driver Uninstallation
echo ============================================================
echo.

rem Check for admin rights
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: This script must be run as Administrator.
    pause
    exit /b 1
)

echo STEP 1: Remove WinUSB driver for our device...
echo.
echo METHOD A - Using pnputil:
for /f "tokens=2" %%a in ('pnputil /enum-drivers ^| findstr /i "ezwriter-winusb"') do (
    echo Found driver package: %%a
    pnputil /delete-driver %%a /uninstall
)
echo.
echo If METHOD A didn't find the driver, try METHOD B:
echo.
echo METHOD B - Using Device Manager:
echo   1. Open Device Manager
echo   2. Find "EZ-Writer II" under "Universal Serial Bus devices"
echo   3. Right-click -> Uninstall device
echo   4. Check "Delete the driver software for this device"
echo   5. Click Uninstall
echo.
echo METHOD C - Using Zadig:
echo   1. Open Zadig as Administrator
echo   2. Select the EZ-Writer device
echo   3. Click the driver dropdown and select "libusb-win32" or the original driver
echo   4. Click "Install Driver" to switch back
echo.
echo Done. You may need to unplug and replug the device.
pause
