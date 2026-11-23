$ErrorActionPreference = 'Stop'
try {
    $name = Get-Content -LiteralPath 'C:\TWIControl\file.txt' -Raw -Encoding UTF8
    $source = 'C:\TWISource'
    $target = Join-Path $env:LOCALAPPDATA 'TwoWindowsInOne\App'
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    & robocopy.exe $source $target /E /XJ /SL /R:0 /W:0 /NFL /NDL /NJH /NJS /NP
    if ($LASTEXITCODE -ge 8) { throw "Не удалось скопировать файлы программы (код $LASTEXITCODE)." }

    $file = Join-Path $target $name
    if ([System.IO.Path]::GetExtension($file) -ieq '.msi') {
        Start-Process msiexec.exe -ArgumentList ('/i "' + $file + '"') -WorkingDirectory $target
    } else {
        Start-Process -FilePath $file -WorkingDirectory $target
    }
} catch {
    Add-Type -AssemblyName PresentationFramework
    [System.Windows.MessageBox]::Show($_.Exception.Message, 'Two Windows in One') | Out-Null
}
