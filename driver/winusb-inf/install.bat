@echo off
setlocal EnableDelayedExpansion

echo ============================================================
echo EZ-Writer II WinUSB Driver Installation
echo ============================================================
echo.
echo This will install the WinUSB driver for the EZ-Writer II.
echo No kernel-mode driver is installed - uses Microsoft's inbox
echo WinUSB.sys which is digitally signed by Microsoft.
echo.

rem Check for admin rights
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: This script must be run as Administrator.
    echo Right-click and select "Run as administrator".
    pause
    exit /b 1
)

echo STEP 1: Checking for existing EZ-Writer driver...
pnputil /enum-drivers | findstr /i "ezwriter ezwinit ezwrit apoader" >nul 2>&1
if !errorlevel! equ 0 (
    echo WARNING: Found existing EZ-Writer kernel driver installed.
    echo You may need to uninstall it first via Device Manager.
    echo The original driver is NOT compatible with Windows 10/11 x64.
    echo.
    choice /C YN /M "Uninstall old driver packages automatically?"
    if !errorlevel! equ 1 (
        echo Removing old EZ-Writer driver packages...
        for /f "tokens=2" %%a in ('pnputil /enum-drivers ^| findstr /i "ezwriter ezwinit ezwrit apoader"') do (
            pnputil /delete-driver %%a /uninstall 2>nul
        )
    )
)

echo.
echo STEP 2: Installing WinUSB driver...
echo.
echo Two recommended methods:
echo.
echo METHOD A - Using pnputil (automatic, but may not work for all USB devices)
echo.
pnputil /add-driver "%~dp0ezwriter-winusb.inf" /install 2>&1
echo.
echo If the above failed (expected for unsigned INF on some systems), try METHOD B:
echo.
echo METHOD B - Using Zadig (recommended):
echo   1. Download Zadig from https://zadig.akeo.ie/
echo   2. Run Zadig as Administrator
echo   3. Options -> List All Devices
echo   4. Select "EZ-Writer II" or "USB\VID_0547&PID_2131"
echo   5. Select "WinUSB (Microsoft)" as driver
echo   6. Click "Replace Driver"
echo   7. Do the same for VID_0548&PID_1005 if it appears
echo.
echo ALTERNATIVE - Using test signing mode:
echo   1. Enable test signing: bcdedit /set testsigning on
echo   2. Reboot
echo   3. Right-click ezwriter-winusb.inf -> Install
echo   4. After done, disable: bcdedit /set testsigning off
echo   5. Reboot
echo.
echo Done. Run 'ezwriter-cli list' to verify.
pause
