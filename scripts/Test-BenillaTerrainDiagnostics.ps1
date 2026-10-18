#requires -Version 5.1
<#
Windows-only authored smoke tests for the sibling collector. No game assets,
network, downloads, administrator rights, or execution-policy changes. Creates
and removes its own temporary fixture tree. Compiles its own fake helper using
Windows' installed .NET Framework csc.exe. Run under powershell.exe AND pwsh.exe.
#>
[CmdletBinding()]
param([string]$Collector = (Join-Path $PSScriptRoot 'Collect-BenillaTerrainDiagnostics.ps1'))
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'These smoke tests require Windows.' }
$Collector = (Resolve-Path -LiteralPath $Collector).ProviderPath
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $compiler)) { $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
if (-not (Test-Path -LiteralPath $compiler)) { throw 'Installed .NET Framework compiler not found; tests were not run.' }
$root = Join-Path ([IO.Path]::GetTempPath()) ('Benilla private path ' + [Guid]::NewGuid().ToString('N'))
$data = Join-Path $root 'Data'; $locale = Join-Path $data 'enUS'
[void][IO.Directory]::CreateDirectory($locale)
$oldMode = [Environment]::GetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', 'Process')
$oldData = [Environment]::GetEnvironmentVariable('BENILLA_DIAG_TEST_DATA', 'Process')
$oldLocation = Get-Location
$passed = 0
try {
    $source = @'
using System;
using System.Threading;
class FakeTerrainHelper {
    static int Main(string[] args) {
        string mode = Environment.GetEnvironmentVariable("BENILLA_DIAG_TEST_MODE") ?? "diagnose";
        if (Array.IndexOf(args, "--help") >= 0) {
            Console.WriteLine("  --inspect    In-memory inspection");
            if (mode != "old" && mode != "nonzero" && mode != "timeout" && mode != "flood")
                Console.WriteLine("  --diagnose   Bounded diagnostic JSON");
            return 0;
        }
        int at = Array.IndexOf(args, "--data");
        if (at < 0 || at + 1 >= args.Length || args[at + 1] != Environment.GetEnvironmentVariable("BENILLA_DIAG_TEST_DATA")) {
            Console.Error.WriteLine("PRIVATE_PATH_ARGUMENT_MISMATCH " + String.Join(" ", args)); return 9;
        }
        if (Array.IndexOf(args, "--output") >= 0) { Console.Error.WriteLine("Unexpected mutating option"); return 10; }
        if (mode == "timeout") { Console.Write("PRIVATE_TIMEOUT_STDOUT"); Console.Error.Write("PRIVATE_TIMEOUT_STDERR"); Thread.Sleep(30000); return 0; }
        if (mode == "flood") {
            string block = new String('x', 4096);
            for (int i = 0; i < 1024; i++) { Console.Write(block); Console.Error.Write(block); }
            return 0;
        }
        if (mode == "nonzero") {
            Console.WriteLine("PRIVATE_UNKNOWN_STDOUT C:\\Private Person\\secret.txt");
            Console.Error.WriteLine("error[terrain.liquid_history]: PRIVATE_ERROR_EXPLANATION C:\\Private Person\\secret.txt");
            Console.Error.WriteLine("error[terrain.PRIVATE_INJECTED_ID]: do not retain this"); return 7;
        }
        if (mode == "old") {
            if (Array.IndexOf(args, "--inspect") < 0) { Console.Error.WriteLine("error[cli.unknown_argument]: private"); return 2; }
            Console.WriteLine("Inspected 1 terrain tiles in memory; no artifacts published");
            Console.WriteLine("map=0 source_epoch=" + new String('a',64));
            Console.WriteLine("tile=48,32 bytes=123 sha256=" + new String('b',64));
            Console.WriteLine("live_install_lease=false gameplay_admitted=false"); return 0;
        }
        if (mode == "badjson") { Console.WriteLine("PRIVATE_NOT_JSON"); return 0; }
        if (Array.IndexOf(args, "--diagnose") < 0) { Console.Error.WriteLine("Wrong mode"); return 11; }
        Console.WriteLine(@"{""format"":""benilla-terrain-diagnostic"",""version"":1,""map"":0,""map_table"":{""selected_row_ordinal"":2,""total_rows"":7},""source_epoch"":""" + new String('c',64) + @""",""live_install_lease"":false,""gameplay_admitted"":false,""private_path"":""PRIVATE_TOP_LEVEL"",""stages"":{""selection"":{""status"":""passed"",""diagnostic_id"":null},""full_capture"":{""status"":""passed"",""diagnostic_id"":null},""area_table"":{""status"":""failed"",""diagnostic_id"":""terrain.PRIVATE_INJECTED_ID""}},""tiles"":[{""x"":48,""y"":32,""raw_adt"":{""bytes"":12345,""sha256"":""" + new String('d',64) + @""",""private"":""PRIVATE_NESTED""},""active_mclq_cells"":3,""liquid_rectangle"":{""x"":0,""y"":1,""width"":129,""height"":128},""missing_liquid_heights"":{""count"":2,""first_x"":128,""first_y"":1},""stages"":{""wdt_selection"":{""status"":""passed"",""diagnostic_id"":null},""adt_structure"":{""status"":""passed"",""diagnostic_id"":null},""liquid"":{""status"":""failed"",""diagnostic_id"":""terrain.liquid_history""},""production"":{""status"":""unavailable"",""diagnostic_id"":null}}}]}");
        return 0;
    }
}
'@
    $sourcePath = Join-Path $root 'AuthoredFakeHelper.cs'; $exe = Join-Path $root 'benilla-native-terrain.exe'
    [IO.File]::WriteAllText($sourcePath, $source)
    $compilerOutput = & $compiler /nologo /target:exe ("/out:" + $exe) $sourcePath 2>&1
    Assert-True ($LASTEXITCODE -eq 0 -and [IO.File]::Exists($exe)) ('Authored fixture compilation failed: ' + ($compilerOutput -join ' '))
    # Small authored MPQ envelopes, not usable archives or copyrighted assets.
    function Write-Envelope([string]$Name, [int]$Version, [int]$Prefix = 0) {
        $size = $(if ($Version -eq 1) { 44 } else { 32 }); $length = $size + 32
        $b = New-Object byte[] ($Prefix + $length)
        [Array]::Copy([BitConverter]::GetBytes([uint32]441536589), 0, $b, $Prefix, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]$size), 0, $b, $Prefix + 4, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]$length), 0, $b, $Prefix + 8, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint16]$Version), 0, $b, $Prefix + 12, 2)
        [Array]::Copy([BitConverter]::GetBytes([uint16]3), 0, $b, $Prefix + 14, 2)
        [Array]::Copy([BitConverter]::GetBytes([uint32]$size), 0, $b, $Prefix + 16, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]($size + 16)), 0, $b, $Prefix + 20, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]1), 0, $b, $Prefix + 24, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]1), 0, $b, $Prefix + 28, 4)
        [IO.File]::WriteAllBytes((Join-Path $data $Name), $b)
    }
    Write-Envelope 'model.MPQ' 0
    Write-Envelope 'dbc.MPQ' 0 512
    Write-Envelope 'terrain.MPQ' 0
    Write-Envelope 'patch.MPQ' 1
    [IO.File]::WriteAllBytes((Join-Path $data 'patch-2.MPQ'), [byte[]]@(77,80,81,26,44))
    [IO.File]::WriteAllText((Join-Path $locale 'locale-enUS.MPQ'), 'authored-not-an-archive')
    [IO.File]::WriteAllText((Join-Path $data 'PRIVATE_ARCHIVE_NAME.MPQ'), 'private-sentinel')
    [IO.File]::WriteAllText((Join-Path $data 'PRIVATE_CONFIG.txt'), 'PRIVATE_CONFIG_CONTENT')
    $before = @{}
    foreach ($f in Get-ChildItem -LiteralPath $root -File -Recurse) { $before[$f.FullName] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash }
    # Keep private unrelated contents locked; the collector must never open them.
    $privateLock = [IO.File]::Open((Join-Path $data 'PRIVATE_CONFIG.txt'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    $mpqLock = [IO.File]::Open((Join-Path $data 'PRIVATE_ARCHIVE_NAME.MPQ'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_DATA', $data, 'Process')
    Set-Location -LiteralPath $root
    function Run-Case([string]$Mode, [int]$Timeout = 8) {
        [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', $Mode, 'Process')
        $output = Join-Path $root ($Mode + '.json')
        # Omit -Data to exercise the relative .\Data default and native quoting of spaces.
        $messages = & $Collector -Helper '.\benilla-native-terrain.exe' -Output $output -TimeoutSeconds $Timeout
        Assert-True ([IO.File]::Exists($output)) ($Mode + ': report not created')
        $raw = [IO.File]::ReadAllText($output)
        Assert-True ($raw -notmatch 'PRIVATE_|Private Person|Benilla private path|[A-Za-z]:\\') ($Mode + ': private text leaked into report')
        Assert-True ($raw.Length -lt 100000) ($Mode + ': report unexpectedly large')
        $r = ConvertFrom-Json -InputObject $raw
        Assert-True ($r.helper.sha256 -eq (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()) ($Mode + ': helper hash mismatch')
        return $r
    }
    $r = Run-Case 'diagnose'
    Assert-True ($r.helper_run.mode -eq '--diagnose' -and $r.helper_run.exit_code -eq 0) 'diagnose: wrong execution mode'
    Assert-True ($r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'diagnose: schema not recognized'
    Assert-True ($r.detailed_diagnostics.report.map_table.selected_row_ordinal -eq 2) 'diagnose: lost selected map row'
    Assert-True ($r.detailed_diagnostics.report.tiles[0].stages.liquid.diagnostic_id -eq 'terrain.liquid_history') 'diagnose: lost safe diagnostic ID'
    Assert-True ($null -eq $r.detailed_diagnostics.report.stages.area_table.diagnostic_id) 'diagnose: unsafe ID retained'
    Assert-True ($r.archive_layout.unrecognized_mpq_names -eq 1) 'layout: unrecognized archive was not counted safely'
    Assert-True ($r.archive_envelopes[1].header.offset -eq 512) 'archive: aligned header not found'
    Assert-True ($r.archive_envelopes[3].header.extended_offsets_all_zero -eq $true) 'archive: version-one extension wrong'
    Assert-True ($r.archive_envelopes[4].status -eq 'header_truncated') 'archive: truncated header not diagnosed'
    $passed++; Write-Output 'PASS: structured diagnostics, relative paths with spaces, safe schema, bounded archive envelopes'
    $r = Run-Case 'old'
    Assert-True ($r.helper_run.mode -eq '--inspect' -and $r.helper_run.exit_code -eq 0) 'old: inspect fallback failed'
    Assert-True ($r.detailed_diagnostics.status -eq 'unavailable_in_older_helper') 'old: unsupported diagnostics not explicit'
    $passed++; Write-Output 'PASS: older helper uses one inspect fallback'
    $r = Run-Case 'nonzero'
    Assert-True ($r.helper_run.exit_code -eq 7 -and $r.helper_run.status -eq 'completed') 'nonzero: exit status lost'
    Assert-True ($r.helper_run.stderr.text -eq 'error[terrain.liquid_history]: [explanation omitted]') 'nonzero: safe error capture incorrect'
    Assert-True ($r.helper_run.stdout.unrecognized_lines_omitted -eq 1) 'nonzero: unknown stdout not omitted'
    $passed++; Write-Output 'PASS: nonzero exit and redacted stdout/stderr'
    $r = Run-Case 'timeout' 1
    Assert-True ($r.helper_run.timed_out -and $r.helper_run.termination_confirmed -and $r.helper_run.elapsed_ms -lt 8000) 'timeout: process not bounded and terminated'
    $passed++; Write-Output 'PASS: timeout termination and bounded drain'
    $r = Run-Case 'flood'
    Assert-True ($r.helper_run.stdout_truncated -and $r.helper_run.stderr_truncated) 'flood: both streams were not bounded'
    Assert-True ($r.helper_run.status -eq 'output_limit_exceeded' -and $r.helper_run.elapsed_ms -lt 8000) 'flood: capture failed or deadlocked'
    $passed++; Write-Output 'PASS: concurrent stdout/stderr flooding does not deadlock'
    $r = Run-Case 'badjson'
    Assert-True ($r.detailed_diagnostics.status -eq 'invalid_json') 'badjson: unknown output not marked invalid'
    $passed++; Write-Output 'PASS: malformed diagnostic JSON is safely omitted'
    $overwrite = $false
    try { & $Collector -Helper '.\benilla-native-terrain.exe' -Output (Join-Path $root 'diagnose.json') | Out-Null } catch { $overwrite = $true }
    Assert-True $overwrite 'overwrite: existing output was not refused'
    $inside = $false
    try { & $Collector -Helper '.\benilla-native-terrain.exe' -Output (Join-Path $data 'forbidden.json') | Out-Null } catch { $inside = $true }
    Assert-True ($inside -and -not [IO.File]::Exists((Join-Path $data 'forbidden.json'))) 'inside Data: output not refused'
    $passed++; Write-Output 'PASS: existing reports and writes inside Data refused'
    $privateLock.Dispose(); $privateLock = $null; $mpqLock.Dispose(); $mpqLock = $null
    foreach ($path in $before.Keys) { Assert-True ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -eq $before[$path]) 'Original fixture file changed' }
    $newFiles = @(Get-ChildItem -LiteralPath $root -File -Recurse | Where-Object { -not $before.ContainsKey($_.FullName) })
    Assert-True ($newFiles.Count -eq 6 -and @($newFiles | Where-Object { $_.Extension -ne '.json' }).Count -eq 0) 'Collector created files other than its explicit JSON reports'
    $passed++; Write-Output 'PASS: input hashes unchanged and no extra files created'
    Write-Output ('PASS: ' + $passed + ' authored smoke groups under PowerShell ' + $PSVersionTable.PSVersion.ToString())
} finally {
    if (Get-Variable -Name privateLock -ErrorAction SilentlyContinue) { if ($null -ne $privateLock) { $privateLock.Dispose() } }
    if (Get-Variable -Name mpqLock -ErrorAction SilentlyContinue) { if ($null -ne $mpqLock) { $mpqLock.Dispose() } }
    Set-Location -LiteralPath $oldLocation.Path
    [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', $oldMode, 'Process')
    [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_DATA', $oldData, 'Process')
    if ([IO.Directory]::Exists($root)) { Remove-Item -LiteralPath $root -Recurse -Force }
}
