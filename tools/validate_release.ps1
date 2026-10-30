param([string]$Executable)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
if (-not $Executable) { $Executable = Join-Path $taskRoot 'dist\AnyCompare.exe' }
$taskExe = (Resolve-Path -LiteralPath $Executable).Path
$taskVersion = ((Select-String -LiteralPath (Join-Path $taskRoot 'Cargo.toml') -Pattern '^version = "([^"]+)"').Matches.Groups[1].Value)
$taskValidation = Join-Path $taskRoot "docs\validation\$taskVersion"
New-Item -ItemType Directory -Path $taskValidation -Force | Out-Null
$taskCases = @(
    @{ Name = 'home'; Args = @(); Directory = 'C:\Users\Altieres\Documents\ChatGPT\anycompare' },
    @{ Name = 'text'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt'); Directory = $taskRoot },
    @{ Name = 'text-modified'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--capture-modified'); Directory = $taskRoot },
    @{ Name = 'folder'; Args = @('--folder', 'tests\fixtures\left', 'tests\fixtures\right'); Directory = $taskRoot },
    @{ Name = 'hex'; Args = @('--hex', 'assets\reference\ico\100.ico', 'assets\reference\ico\101.ico'); Directory = $taskRoot },
    @{ Name = 'hex-aligned'; Args = @('--hex', 'docs\reference\controlled-hex-replay\inserted-left.bin', 'docs\reference\controlled-hex-replay\inserted-right.bin'); Directory = $taskRoot },
    @{ Name = 'hex-empty'; Args = @('--hex', 'docs\reference\controlled-hex-replay\empty-left.bin', 'docs\reference\controlled-hex-replay\empty-right.bin'); Directory = $taskRoot },
    @{ Name = 'hex-duplicate-anchor'; Args = @('--hex', 'docs\reference\controlled-hex-audit-2026-10-30\duplicate-anchor-left.bin', 'docs\reference\controlled-hex-audit-2026-10-30\duplicate-anchor-right.bin'); Directory = $taskRoot },
    @{ Name = 'home-en'; Args = @('--language', 'en'); Directory = $taskRoot },
    @{ Name = 'text-en'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', 'en'); Directory = $taskRoot },
    @{ Name = 'search-pt'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', 'pt-BR', '--capture-find', 'texto'); Directory = $taskRoot },
    @{ Name = 'search-en'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', 'en', '--capture-find', 'text'); Directory = $taskRoot },
    @{ Name = 'paths-history'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', 'pt-BR', '--capture-menu', 'Arquivo esquerdo'); Directory = $taskRoot },
    @{ Name = 'search-history'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', 'pt-BR', '--capture-find', 'texto', '--capture-menu', 'Localizar'); Directory = $taskRoot },
    @{ Name = 'folder-en'; Args = @('--folder', 'tests\fixtures\left', 'tests\fixtures\right', '--language', 'en'); Directory = $taskRoot },
    @{ Name = 'hex-en'; Args = @('--hex', 'assets\reference\ico\100.ico', 'assets\reference\ico\101.ico', '--language', 'en'); Directory = $taskRoot }
)
foreach ($taskMenu in @('Sessão', 'Ferramentas', 'Editar')) {
    foreach ($taskLanguage in @('pt-BR', 'en')) {
        $taskCases += @{ Name = "menu-$taskMenu-$taskLanguage"; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', $taskLanguage, '--capture-menu', $taskMenu); Directory = $taskRoot }
    }
}
$taskCases += @(
    @{ Name = 'tabs-text-pt'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--capture-tabs', '--size', '958', '1000', '--language', 'pt-BR'); Directory = $taskRoot },
    @{ Name = 'tabs-text-en'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--capture-tabs', '--size', '958', '1000', '--language', 'en'); Directory = $taskRoot },
    @{ Name = 'tabs-text-modified'; Args = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--capture-tabs', '--capture-modified', '--size', '958', '1000'); Directory = $taskRoot },
    @{ Name = 'tabs-hex'; Args = @('--hex', 'assets\reference\ico\100.ico', 'assets\reference\ico\101.ico', '--capture-tabs', '--size', '958', '1000'); Directory = $taskRoot },
    @{ Name = 'folder-drop-pair'; Args = @('--capture-drop', 'tests\fixtures\left', 'tests\fixtures\right'); Directory = $taskRoot },
    @{ Name = 'folder-drop-separate'; Args = @('--capture-drop', 'tests\fixtures\left', 'tests\fixtures\right', '--capture-drop-separate'); Directory = $taskRoot },
    @{ Name = 'folder-drop-en'; Args = @('--capture-drop', 'tests\fixtures\left', 'tests\fixtures\right', '--language', 'en'); Directory = $taskRoot },
    @{ Name = 'folder-nested'; Args = @('--folder', 'docs\reference\controlled-folder-replay-2026-10-30\nested\left', 'docs\reference\controlled-folder-replay-2026-10-30\nested\right'); Directory = $taskRoot },
    @{ Name = 'folder-type-conflict'; Args = @('--folder', 'docs\reference\controlled-folder-replay-2026-10-30\file-directory\left', 'docs\reference\controlled-folder-replay-2026-10-30\file-directory\right'); Directory = $taskRoot },
    @{ Name = 'folder-case'; Args = @('--folder', 'docs\reference\controlled-folder-replay-2026-10-30\case-parent\left', 'docs\reference\controlled-folder-replay-2026-10-30\case-parent\right'); Directory = $taskRoot },
    @{ Name = 'folder-matches'; Args = @('--folder', 'docs\reference\controlled-folder-replay-2026-10-30\case-parent\left', 'docs\reference\controlled-folder-replay-2026-10-30\case-parent\right', '--capture-menu', 'Barra: Iguais'); Directory = $taskRoot }
)
$taskUserFiles = @('C:\Users\Altieres\Desktop\resumo.txt', 'C:\Users\Altieres\Desktop\resumo 1.txt')
foreach ($taskLanguage in @('pt-BR', 'en')) {
    foreach ($taskView in @('text', 'folder', 'hex')) {
        $taskPair = switch ($taskView) {
            'text' { @('tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt') }
            'folder' { @('tests\fixtures\left', 'tests\fixtures\right') }
            'hex' { @('assets\reference\ico\100.ico', 'assets\reference\ico\101.ico') }
        }
        $taskCases += @{ Name = "sessions-$taskView-$taskLanguage"; Args = @("--$taskView") + $taskPair + @('--language', $taskLanguage, '--capture-menu', 'Sessões'); Directory = $taskRoot }
    }
}
$taskEmptyProfile = Join-Path $taskRoot '.codex\validation-empty-profile'
New-Item -ItemType Directory -Path $taskEmptyProfile -Force | Out-Null
foreach ($taskLanguage in @('pt-BR', 'en')) {
    foreach ($taskState in @('collapsed', 'expanded')) {
        $taskArgs = @('--text', 'tests\fixtures\left\example.txt', 'tests\fixtures\right\example.txt', '--language', $taskLanguage, '--capture-menu', 'Sessões')
        if ($taskState -eq 'expanded') { $taskArgs += @('--capture-submenu', 'Sessões Novo') }
        $taskCases += @{ Name = "native-sessions-$taskState-$taskLanguage"; Args = $taskArgs; Directory = $taskRoot; EmptyHistory = $true }
    }
}
$taskBefore = @()
if (($taskUserFiles | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }).Count -eq 2) {
    $taskBefore = @($taskUserFiles | ForEach-Object { Get-FileHash -LiteralPath $_ -Algorithm SHA256 })
    foreach ($mode in @('editing', 'read-only', 'differences')) {
        $taskArgs = @('--text') + $taskUserFiles + @('--size', '960', '1000')
        if ($mode -ne 'editing') { $taskArgs += "--$mode" }
        $taskCases += @{ Name = "text-$mode"; Args = $taskArgs + @('--language', 'pt-BR'); Directory = $taskRoot }
    }
    foreach ($taskMode in @('all', 'differences')) {
        $taskArgs = @('--text', $taskUserFiles[1], $taskUserFiles[0], '--read-only', '--language', 'pt-BR', '--size', '1616', '838')
        if ($taskMode -eq 'differences') { $taskArgs += '--differences' }
        $taskCases += @{ Name = "text-dividers-$taskMode"; Args = $taskArgs; Directory = $taskRoot }
    }
}
$taskResults = @()
foreach ($case in $taskCases) {
    $taskCapture = Join-Path $taskValidation "$($case.Name).png"
    $taskStdout = Join-Path $taskValidation "$($case.Name).out.log"
    $taskStderr = Join-Path $taskValidation "$($case.Name).err.log"
    $taskArguments = ($case.Args + @('--capture', $taskCapture) | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $taskOriginalAppData = $env:APPDATA
    try {
        if ($case.EmptyHistory) { $env:APPDATA = $taskEmptyProfile }
        $taskProcess = Start-Process -FilePath $taskExe -ArgumentList $taskArguments -WorkingDirectory $case.Directory -WindowStyle Hidden -RedirectStandardOutput $taskStdout -RedirectStandardError $taskStderr -PassThru
    } finally { $env:APPDATA = $taskOriginalAppData }
    # Windows PowerShell 5.1 only reports ExitCode if the handle was opened before the process exits.
    $null = $taskProcess.Handle
    if (-not $taskProcess.WaitForExit(30000)) {
        $taskProcess.Kill()
        throw "Capture timed out: $($case.Name)"
    }
    $taskProcess.Refresh()
    $taskResults += [ordered]@{
        screen = $case.Name; exited = $taskProcess.HasExited; exit_code = $taskProcess.ExitCode
        stderr_bytes = (Get-Item -LiteralPath $taskStderr).Length
        capture = "docs/validation/$taskVersion/$($case.Name).png"; working_directory = $case.Directory
    }
    if ($taskProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $taskCapture)) { throw "Capture failed: $($case.Name)" }
}
$taskUserChecks = @()
foreach ($before in $taskBefore) {
    $after = Get-FileHash -LiteralPath $before.Path -Algorithm SHA256
    $taskUserChecks += [ordered]@{ path = $before.Path; sha256_before = $before.Hash; sha256_after = $after.Hash; unchanged = ($before.Hash -eq $after.Hash) }
    if ($before.Hash -ne $after.Hash) { throw "Input file changed during validation: $($before.Path)" }
}
$taskGeometry = @()
if ($taskBefore.Count -eq 2) {
    $taskCapture = Join-Path $taskValidation 'text-dividers-all.png'
    foreach ($taskCheck in @(
        @{ Script = 'compare_text_dividers.py'; Report = 'dividers.json' },
        @{ Script = 'compare_text_bottom.py'; Report = 'geometry.json' }
    )) {
        $taskJson = Join-Path $taskValidation $taskCheck.Report
        & python -I (Join-Path $PSScriptRoot $taskCheck.Script) $taskCapture --output $taskJson *> (Join-Path $taskValidation "$($taskCheck.Script).log")
        if ($LASTEXITCODE -ne 0) { throw "Visual geometry failed: $($taskCheck.Script)" }
        $taskGeometry += [ordered]@{ report = "docs/validation/$taskVersion/$($taskCheck.Report)"; passed = $true }
    }
}
$taskPopupReport = Join-Path $taskValidation 'popup-geometry.json'
& python -I (Join-Path $PSScriptRoot 'compare_session_popup.py') $taskValidation --output $taskPopupReport *> (Join-Path $taskValidation 'compare_session_popup.py.log')
if ($LASTEXITCODE -ne 0) { throw 'Session popup geometry failed' }
$taskGeometry += [ordered]@{ report = "docs/validation/$taskVersion/popup-geometry.json"; passed = $true }
$taskReport = [ordered]@{
    date = (Get-Date -Format 'yyyy-MM-dd'); version = $taskVersion; binary = $taskExe
    sha256 = (Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash
    bytes = (Get-Item -LiteralPath $taskExe).Length; screens = $taskResults; input_integrity = $taskUserChecks; visual_geometry = $taskGeometry
}
$taskReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskValidation 'release-smoke.json') -Encoding UTF8
$taskReport | ConvertTo-Json -Depth 8
