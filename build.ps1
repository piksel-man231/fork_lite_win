$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

if (-not (Test-Path 'bin\sing-box.exe')) {
    throw 'Положите sing-box.exe в bin перед сборкой.'
}
if (-not (Test-Path 'bin\libcronet.dll')) { throw 'Положите libcronet.dll в bin.' }
if (-not (Test-Path 'bin\xray.exe')) { throw 'Положите xray.exe в bin.' }
if (-not (Test-Path 'bin\wintun.dll')) { throw 'Положите wintun.dll в bin.' }

python -m pip install -r requirements.txt
$binaryArgs = @(
    '--add-binary', 'bin\sing-box.exe;bin',
    '--add-binary', 'bin\libcronet.dll;bin',
    '--add-binary', 'bin\xray.exe;bin',
    '--add-binary', 'bin\wintun.dll;bin'
)
python -m PyInstaller --noconfirm --clean --onefile --windowed --uac-admin --hidden-import pystray._win32 --name ForkLiteWin --icon Assets\AppIcon.ico --add-data 'Assets\AppIcon.ico;Assets' --add-data 'Assets\AppIcon.png;Assets' @binaryArgs app.py
Write-Host 'Готово: dist\ForkLiteWin.exe'
