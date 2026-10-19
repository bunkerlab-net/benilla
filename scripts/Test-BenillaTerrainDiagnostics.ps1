#requires -Version 5.1
<#
Windows-only authored smoke tests for the sibling collector. No game assets,
network, downloads, administrator rights, or execution-policy changes. Creates
and removes its own temporary fixture tree. Compiles its own fake helper and player using
Windows' installed .NET Framework csc.exe. Run under powershell.exe AND pwsh.exe.
The authored player emits v3 navigation records without launching a navigation
helper. These tests validate collector filtering and process bounds, not the
production player's sibling-helper trust/path checks or navigation generation.
#>
[CmdletBinding()]
param([string]$Collector = '')
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
function Assert-True([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Assert-ReportSerialization([string]$Path, [string]$Raw, [string]$Case) {
    $utf8 = New-Object Text.UTF8Encoding($false)
    $byteCount = $utf8.GetByteCount($Raw)
    Assert-True ($byteCount -lt 100000) ($Case + ': report unexpectedly large (' + $byteCount + ' UTF-8 bytes)')
    Assert-True ((New-Object IO.FileInfo($Path)).Length -eq $byteCount) ($Case + ': report is not BOM-free UTF-8')
    Assert-True ($Raw.EndsWith("`r`n")) ($Case + ': report terminator changed')
    Assert-True ($Raw.Substring(0, $Raw.Length - 2) -notmatch '[\r\n]') ($Case + ': report contains pretty-print line breaks')
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'These smoke tests require Windows.' }
# Resolve script-relative defaults only after parameter binding (Windows PowerShell 5.1).
if ([string]::IsNullOrWhiteSpace($Collector)) {
    $scriptDirectory = $PSScriptRoot
    if ([string]::IsNullOrWhiteSpace($scriptDirectory)) {
        $scriptFile = $MyInvocation.MyCommand.Path
        if ([string]::IsNullOrWhiteSpace($scriptFile)) { throw 'Cannot locate this test script; pass -Collector explicitly.' }
        $scriptDirectory = [IO.Path]::GetDirectoryName($scriptFile)
    }
    $Collector = Join-Path $scriptDirectory 'Collect-BenillaTerrainDiagnostics.ps1'
}
$Collector = (Resolve-Path -LiteralPath $Collector).ProviderPath
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $compiler)) { $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
if (-not (Test-Path -LiteralPath $compiler)) { throw 'Installed .NET Framework compiler not found; tests were not run.' }
$root = Join-Path ([IO.Path]::GetTempPath()) ('Benilla private path ' + [char]0x00E9 + [char]0x4E2D + ' ' + [Guid]::NewGuid().ToString('N'))
$data = Join-Path $root 'Data'; $locale = Join-Path $data 'enUS'
[void][IO.Directory]::CreateDirectory($locale)
$oldMode = [Environment]::GetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', 'Process')
$oldData = [Environment]::GetEnvironmentVariable('BENILLA_DIAG_TEST_DATA', 'Process')
$oldCollisionMode = [Environment]::GetEnvironmentVariable('BENILLA_COLLISION_TEST_MODE', 'Process')
$reports = New-Object 'Collections.Generic.List[string]'
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
        Console.WriteLine(@"{""format"":""benilla-terrain-diagnostic"",""version"":1,""map"":0,""map_table"":{""selected_row_ordinal"":2,""total_rows"":7},""source_epoch"":""" + new String('c',64) + @""",""live_install_lease"":false,""gameplay_admitted"":false,""private_path"":""PRIVATE_TOP_LEVEL"",""stages"":{""selection"":{""status"":""passed"",""diagnostic_id"":null},""full_capture"":{""status"":""passed"",""diagnostic_id"":null},""area_table"":{""status"":""failed"",""diagnostic_id"":""terrain.PRIVATE_INJECTED_ID""}},""tiles"":[{""x"":48,""y"":32,""raw_adt"":{""bytes"":12345,""sha256"":""" + new String('d',64) + @""",""private"":""PRIVATE_NESTED""},""active_mclq_cells"":3,""history"":{""consulted_adts"":2,""consulted_wdts"":1,""absent_members"":1,""initial_zero_samples"":0,""private"":""PRIVATE_HISTORY""},""liquid_rectangle"":{""x"":0,""y"":1,""width"":129,""height"":128},""missing_liquid_heights"":{""count"":2,""first_x"":128,""first_y"":1},""stages"":{""wdt_selection"":{""status"":""passed"",""diagnostic_id"":null},""adt_structure"":{""status"":""passed"",""diagnostic_id"":null},""liquid"":{""status"":""failed"",""diagnostic_id"":""terrain.liquid_history""},""history_resolution"":{""status"":""passed"",""diagnostic_id"":null},""production"":{""status"":""passed"",""diagnostic_id"":null}}}]}");
        return 0;
    }
}
'@
    $sourcePath = Join-Path $root 'AuthoredFakeHelper.cs'; $exe = Join-Path $root 'benilla-native-terrain.exe'
    [IO.File]::WriteAllText($sourcePath, $source)
    $compilerOutput = & $compiler /nologo /target:exe ("/out:" + $exe) $sourcePath 2>&1
    Assert-True ($LASTEXITCODE -eq 0 -and [IO.File]::Exists($exe)) ('Authored fixture compilation failed: ' + ($compilerOutput -join ' '))
    $playerSource = @'
using System;
using System.Threading;
class FakeCollisionPlayer {
    static int map, x, y;
    static string Record(string stage) {
        return "{\"format\":\"benilla-collision-diagnostics-v3\",\"stage\":\"" + stage + "\",\"status\":\"passed\",\"map\":" + map + ",\"tile_x\":" + x + ",\"tile_y\":" + y + ",\"elapsed_ms\":1,\"code\":null,\"source_stage\":null,\"metrics\":{},\"hashes\":{},\"probe\":null,\"gameplay_admitted\":false}";
    }
    static string Probe(int index) {
        return Record("probe").Replace("\"probe\":null", "\"probe\":{\"index\":" + index + ",\"kind\":\"height\",\"from_bits\":[0,2147483648,4294967295],\"to_bits\":null,\"result\":\"miss\",\"value_bits\":null}");
    }
    static string StaticProbe(int index) {
        return Probe(index).Replace("\"stage\":\"probe\"", "\"stage\":\"static_probe\"").Replace("\"kind\":\"height\"", "\"kind\":\"static_height\"");
    }
    static string NavigationProbe(int index) {
        return Record("navigation_probe").Replace("\"metrics\":{}", "\"metrics\":{\"polygon_ref_low\":4294967295,\"polygon_ref_high\":0}").Replace("\"probe\":null", "\"probe\":{\"index\":" + index + ",\"kind\":\"nearest\",\"from_bits\":[0,2147483648,4294967295],\"to_bits\":[4294967295,1,0],\"result\":\"nearest\",\"value_bits\":null}");
    }
    static string NavigationMiss(int index) {
        return NavigationProbe(index).Replace("\"metrics\":{\"polygon_ref_low\":4294967295,\"polygon_ref_high\":0}", "\"metrics\":{}").Replace("\"to_bits\":[4294967295,1,0]", "\"to_bits\":null").Replace("\"result\":\"nearest\"", "\"result\":\"miss\"");
    }
    static string NavigationError(int index) {
        return NavigationMiss(index).Replace("\"passed\"", "\"failed\"").Replace("\"result\":\"miss\"", "\"result\":\"error\"").Replace("\"code\":null", "\"code\":\"navigation.native_query\"");
    }
    static string Unavailable(string stage) {
        return Record(stage).Replace("\"passed\"", "\"unavailable\"").Replace("\"code\":null", "\"code\":\"collision.prerequisite\"");
    }
    static string NavigationGeneration(int status) {
        return Record("navigation_generation").Replace("\"metrics\":{}", "\"metrics\":{\"native_peak_bytes\":65536,\"navigation_output_bytes\":" + (status == 0 ? 456 : 0) + ",\"navigation_polygons\":" + (status == 0 ? 42 : status == 2 ? 625 : 0) + ",\"navigation_status\":" + status + "}");
    }
    static string NavigationValidation() {
        return Record("navigation_validation").Replace("\"hashes\":{}", "\"hashes\":{\"navigation_output\":\"" + new String('e',64) + "\"}");
    }
    static string Terrain() {
        return Record("terrain_capture").Replace("\"metrics\":{}", "\"metrics\":{\"terrain_tiles\":1,\"terrain_bytes\":1234}").Replace("\"hashes\":{}", "\"hashes\":{\"terrain_source_epoch\":\"" + new String('b',64) + "\"}");
    }
    static void Write(string text) { Console.WriteLine(text); Console.Out.Flush(); }
    static int Main(string[] args) {
        if (args.Length != 9 || args[0] != "diagnose-collision" || args[1] != "--data" || args[2] != Environment.GetEnvironmentVariable("BENILLA_DIAG_TEST_DATA") || args[3] != "--map" || args[5] != "--tile" || args[7] != "--timeout-seconds") {
            Console.Error.WriteLine("PRIVATE_PLAYER_ARGUMENT_MISMATCH " + String.Join(" ", args)); return 9;
        }
        map = Int32.Parse(args[4]); string[] tile = args[6].Split(','); x = Int32.Parse(tile[0]); y = Int32.Parse(tile[1]);
        string mode = Environment.GetEnvironmentVariable("BENILLA_COLLISION_TEST_MODE") ?? "success";
        Console.Error.WriteLine("PRIVATE_PLAYER_STDERR C:\\Private Person\\secret.txt");
        if (mode == "legacy_version" || mode == "legacy_v2") {
            string version = mode == "legacy_version" ? "v1" : "v2";
            Write(Record("selection").Replace("benilla-collision-diagnostics-v3", "benilla-collision-diagnostics-" + version));
            Write(Terrain().Replace("benilla-collision-diagnostics-v3", "benilla-collision-diagnostics-" + version)); return 0;
        }
        if (mode == "mixed_version") {
            Write(Record("selection"));
            Write(Terrain().Replace("benilla-collision-diagnostics-v3", "benilla-collision-diagnostics-v1"));
            Write(Record("capture").Replace("benilla-collision-diagnostics-v3", "benilla-collision-diagnostics-v2"));
            Write(Terrain()); Write(Record("capture")); return 0;
        }
        if (mode == "newline_budget") {
            Write(Record("selection").PadRight(8192)); Write(Terrain().PadRight(8192));
            for (int i = 0; i < 30; i++) Write(Record("probe").PadRight(8192)); return 0;
        }
        Write(Record("selection"));
        Write(Terrain());
        string capture = Record("capture");
        if (mode == "failure") {
            Write(capture.Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"vmap.source.member_read\"").Replace("\"source_stage\":null", "\"source_stage\":\"AdtTraversal\""));
            Write(Record("placement").Replace("\"passed\"", "\"unavailable\"").Replace("\"code\":null", "\"code\":\"collision.prerequisite\""));
            foreach (string stage in new string[] { "assembly", "validation", "probe", "composition", "static_probe", "composition_freshness", "navigation_neighbor_0", "navigation_neighbor_1", "navigation_neighbor_2", "navigation_neighbor_3", "navigation_terrain", "navigation_input", "navigation_generation", "navigation_validation", "navigation_probe", "navigation_freshness", "freshness" })
                Write(Record(stage).Replace("\"passed\"", "\"unavailable\"").Replace("\"code\":null", "\"code\":\"collision.prerequisite\""));
            Write(Record("complete").Replace("\"passed\"", "\"failed\"")); return 7;
        }
        if (mode == "soft_timeout") {
            Thread.Sleep((Int32.Parse(args[8]) + 1) * 1000);
            Write(capture.Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"collision.timeout\""));
            Write(Record("complete").Replace("\"passed\"", "\"failed\"")); return 7;
        }
        if (mode == "timeout") { Write(capture); Console.Write("{\"PRIVATE_PARTIAL\":"); Console.Out.Flush(); Thread.Sleep(30000); return 0; }
        if (mode == "flood") {
            string block = new String('x', 4096);
            for (int i = 0; i < 1024; i++) { Console.Write(block); Console.Error.Write(block); }
            return 0;
        }
        if (mode == "oversized") { Write(new String('x', 16385)); Write(capture); return 0; }
        if (mode == "malformed") { Write("PRIVATE_NOT_JSON C:\\Private Person\\secret.txt"); Write(capture); return 0; }
        if (mode == "invalid_schema") {
            string[] invalid = {
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":\"PRIVATE_METRIC\"}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":true}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":1.0}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":1e0}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":-1}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":9223372036854775808}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"PRIVATE_MEMBER_NAME\":0}"),
                capture.Replace("\"metrics\":{}", "\"metrics\":[]"),
                capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":1,\"member_lookups\":2}"),
                capture.Replace("\"hashes\":{}", "\"hashes\":{\"source_epoch\":\"PRIVATE_HASH\"}"),
                capture.Replace("\"hashes\":{}", "\"hashes\":{\"source_epoch\":true}"),
                capture.Replace("\"hashes\":{}", "\"hashes\":{\"PRIVATE_HASH_NAME\":\"" + new String('a',64) + "\"}"),
                capture.Replace("\"hashes\":{}", "\"hashes\":null"),
                capture.Replace("\"code\":null", "\"code\":\"vmap.PRIVATE_INJECTED_ID\""),
                capture.Replace("\"source_stage\":null", "\"source_stage\":\"PRIVATE_SOURCE\""),
                capture.Replace("\"status\":\"passed\"", "\"status\":\"Passed\""),
                capture.Replace("\"map\":" + map, "\"map\":999"),
                capture.Replace("\"map\":" + map, "\"map\":1000"),
                capture.Replace("\"tile_x\":" + x, "\"tile_x\":64"),
                capture.Replace("\"tile_y\":" + y, "\"tile_y\":1"),
                capture.Replace("\"map\":" + map, "\"map\":\"0\""),
                capture.Replace("\"map\":" + map, "\"map\":false"),
                capture.Replace("\"map\":" + map, "\"map\":[0]"),
                capture.Replace("\"elapsed_ms\":1", "\"elapsed_ms\":null"),
                capture.Replace("\"elapsed_ms\":1", "\"elapsed_ms\":9223372036854775808"),
                capture.Replace("\"elapsed_ms\":1,", ""),
                capture.Replace("\"gameplay_admitted\":false", "\"gameplay_admitted\":true"),
                capture.Replace("\"gameplay_admitted\":false", "\"gameplay_admitted\":\"false\""),
                capture.Replace("\"map\":" + map, "\"map\":0,\"map\":0"),
                capture.Replace("\"map\":" + map, "\"map\":0,\"Map\":0"),
                capture.Replace("\"map\":", "\"m\\u0061p\":"),
                capture.Replace("\"map\":", "map:"),
                capture.Replace('"', '\''),
                capture.Substring(0,capture.Length-1) + ",}",
                capture.Replace("\"map\":", "/*PRIVATE_COMMENT*/\"map\":"),
                capture.Substring(0,capture.Length-1) + ",\"PRIVATE_PATH\":\"C:\\\\Private Person\\\\secret.txt\"}",
                capture.Replace("\"probe\":null", "\"probe\":{}"),
                Probe(16),
                Probe(0).Replace("4294967295", "4294967296"),
                Probe(0).Replace("[0,2147483648,4294967295]", "[0,1]"),
                Probe(0).Replace("\"to_bits\":null", "\"to_bits\":[0,1,\"PRIVATE_BIT\"]"),
                Probe(0).Replace("\"value_bits\":null", "\"value_bits\":4294967296"),
                Probe(0).Replace("\"result\":\"miss\"", "\"result\":\"PRIVATE_RESULT\""),
                Probe(0).Replace("\"kind\":\"height\"", "\"kind\":\"PRIVATE_KIND\""),
                Probe(0).Replace("\"index\":0", "\"index\":0,\"PRIVATE_PROBE\":0"),
                "[" + capture + "]"
            };
            foreach (string line in invalid) Write(line);
            Write(capture); return 0;
        }
        if (mode == "invalid_static_schema") {
            string[] invalid = {
                StaticProbe(9),
                StaticProbe(0).Replace("\"static_height\"", "\"height\""),
                StaticProbe(0).Replace("\"static_height\"", "\"segment\""),
                Probe(0).Replace("\"height\"", "\"static_height\""),
                StaticProbe(0).Replace("\"to_bits\":null", "\"to_bits\":[0,1,2]"),
                StaticProbe(0).Replace("\"miss\"", "\"hit\""),
                StaticProbe(0).Replace("\"index\":0", "\"index\":false"),
                StaticProbe(0).Replace("\"value_bits\":null", "\"value_bits\":\"PRIVATE_STATIC_VALUE\""),
                Record("composition").Replace("\"hashes\":{}", "\"hashes\":{\"composition\":\"PRIVATE_COMPOSITION\"}"),
                Terrain().Replace("\"terrain_tiles\":1", "\"terrain_tiles\":true"),
                Terrain().Replace("\"terrain_bytes\":1234", "\"terrain_bytes\":-1"),
                Record("complete").Replace("\"metrics\":{}", "\"metrics\":{\"completed_static_probes\":\"PRIVATE_COUNT\"}")
            };
            foreach (string line in invalid) Write(line);
            Write(capture); Write(Record("composition")); Write(StaticProbe(0)); Write(StaticProbe(8)); return 0;
        }
        if (mode == "invalid_navigation_schema") {
            string[] invalid = {
                NavigationProbe(9),
                NavigationProbe(0).Replace("\"kind\":\"nearest\"", "\"kind\":\"height\""),
                NavigationProbe(0).Replace("\"kind\":\"nearest\"", "\"kind\":\"static_height\""),
                Probe(0).Replace("\"kind\":\"height\"", "\"kind\":\"nearest\""),
                NavigationProbe(0).Replace("\"result\":\"nearest\"", "\"result\":\"height\""),
                NavigationProbe(0).Replace("\"result\":\"nearest\"", "\"result\":\"miss\""),
                NavigationProbe(0).Replace("\"result\":\"nearest\"", "\"result\":\"error\""),
                NavigationProbe(0).Replace("\"to_bits\":[4294967295,1,0]", "\"to_bits\":null"),
                NavigationProbe(0).Replace("\"to_bits\":[4294967295,1,0]", "\"to_bits\":[0,1]"),
                NavigationProbe(0).Replace("\"to_bits\":[4294967295,1,0]", "\"to_bits\":[4294967296,1,0]"),
                NavigationProbe(0).Replace("\"to_bits\":[4294967295,1,0]", "\"to_bits\":[false,1,0]"),
                NavigationProbe(0).Replace("\"from_bits\":[0,2147483648,4294967295]", "\"from_bits\":null"),
                NavigationProbe(0).Replace("\"from_bits\":[0,2147483648,4294967295]", "\"from_bits\":[0,1,\"PRIVATE_BIT\"]"),
                NavigationProbe(0).Replace("\"value_bits\":null", "\"value_bits\":0"),
                NavigationProbe(0).Replace("\"index\":0", "\"index\":true"),
                NavigationProbe(0).Replace("\"passed\"", "\"unavailable\""),
                NavigationProbe(0).Replace("\"passed\"", "\"failed\""),
                NavigationProbe(0).Replace("\"code\":null", "\"code\":\"navigation.native_query\""),
                NavigationError(0).Replace("\"failed\"", "\"passed\""),
                NavigationError(0).Replace("\"to_bits\":null", "\"to_bits\":[0,1,2]"),
                NavigationMiss(0).Replace("\"value_bits\":null", "\"value_bits\":0"),
                Record("navigation_probe"),
                Record("navigation_probe").Replace("\"passed\"", "\"failed\""),
                Record("navigation_probe").Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"collision.query\""),
                Record("navigation_input").Replace("\"metrics\":{}", "\"metrics\":{\"neighbor_present\":2}"),
                Record("navigation_input").Replace("\"metrics\":{}", "\"metrics\":{\"neighbor_index\":4}"),
                NavigationGeneration(3),
                NavigationProbe(0).Replace("\"polygon_ref_low\":4294967295", "\"polygon_ref_low\":4294967296"),
                NavigationProbe(0).Replace("\"polygon_ref_high\":0", "\"polygon_ref_high\":4294967296"),
                NavigationProbe(0).Replace("\"polygon_ref_high\":0", "\"polygon_ref_high\":false"),
                Record("complete").Replace("\"metrics\":{}", "\"metrics\":{\"completed_navigation_probes\":10}"),
                Record("navigation_input").Replace("\"metrics\":{}", "\"metrics\":{\"request_bytes\":\"PRIVATE_BYTES\"}"),
                Record("navigation_input").Replace("\"metrics\":{}", "\"metrics\":{\"solid_vertices\":-1}"),
                Record("navigation_input").Replace("\"hashes\":{}", "\"hashes\":{\"navigation_input\":\"PRIVATE_HASH\"}"),
                NavigationValidation().Replace(new String('e',64), new String('e',63)),
                NavigationValidation().Replace(new String('e',64), new String('E',64)),
                NavigationGeneration(0).Replace("\"code\":null", "\"code\":\"navigation.PRIVATE_ERROR\"")
            };
            foreach (string line in invalid) Write(line);
            Write(capture); Write(NavigationProbe(0)); Write(NavigationMiss(1)); Write(NavigationError(8)); return 0;
        }
        if (mode == "order") {
            Write(capture); Write(capture); Write(Record("selection"));
            Write(Probe(0)); Write(Probe(0)); Write(Probe(1)); Write(Record("composition"));
            Write(StaticProbe(0)); Write(StaticProbe(0)); Write(StaticProbe(1));
            Write(NavigationProbe(0)); Write(NavigationProbe(0)); Write(NavigationProbe(1));
            Write(StaticProbe(2)); Write(Record("freshness")); Write(Record("assembly")); return 0;
        }
        if (mode == "budget") {
            Write(capture.PadRight(16000));
            Write(Record("placement").PadRight(16000)); Write(Record("assembly").PadRight(16000)); Write(Record("validation").PadRight(16000));
            for (int i = 0; i < 16; i++) Write(Probe(i).PadRight(16000));
            Write(Record("freshness").PadRight(16000)); Write(Record("complete").PadRight(16000)); return 0;
        }
        if (mode == "record_limit") { for (int i = 0; i < 72; i++) Write(Record("probe")); return 0; }
        if (mode == "source_codes") {
            Write(capture.Replace("\"code\":null", "\"code\":\"vmap.source.header_version\"").Replace("\"source_stage\":null", "\"source_stage\":\"OpenSource\""));
            Write(Record("placement").Replace("\"code\":null", "\"code\":\"vmap.source_changed_during_preparation\""));
            Write(Record("assembly").Replace("\"code\":null", "\"code\":\"vmap.bih_limit\""));
            Write(Record("validation").Replace("\"code\":null", "\"code\":\"vmap.model_write.allocation\""));
            Write(Record("composition_freshness").Replace("\"code\":null", "\"code\":\"terrain.source_changed\""));
            Write(Record("complete").Replace("\"code\":null", "\"code\":\"collision.unknown_error\"")); return 0;
        }
        Write(capture.Replace("\"metrics\":{}", "\"metrics\":{\"member_lookups\":3,\"read_work_bytes\":9223372036854775807}").Replace("\"hashes\":{}", "\"hashes\":{\"source_epoch\":\"" + new String('a',64) + "\"}"));
        Write(Record("placement"));
        Write(Record("assembly").Replace("\"metrics\":{}", "\"metrics\":{\"tree_bytes\":32,\"tile_bytes\":64,\"model_bytes\":128,\"serialized_bytes\":224,\"tiles\":1,\"models\":2}"));
        Write(Record("validation"));
        Write(Probe(0));
        Write(Probe(1).Replace("\"height\"", "\"segment\"").Replace("\"to_bits\":null", "\"to_bits\":[0,1,2]").Replace("\"miss\"", "\"hit\"").Replace("\"value_bits\":null", "\"value_bits\":4294967295"));
        for (int i = 2; i < 9; i++) Write(Probe(i));
        Write(Record("composition").Replace("\"hashes\":{}", "\"hashes\":{\"composition\":\"" + new String('c',64) + "\"}"));
        for (int i = 0; i < 9; i++) Write(StaticProbe(i));
        Write(Record("composition_freshness"));
        for (int i = 0; i < 4; i++) {
            string neighbor = Record("navigation_neighbor_" + i).Replace("\"metrics\":{}", "\"metrics\":{\"neighbor_index\":" + i + ",\"neighbor_present\":" + (i % 2) + "}");
            if (mode == "navigation_neighbor_failure" && i == 1)
                neighbor = neighbor.Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"source.open_patch_chain\"");
            Write(neighbor);
        }
        if (mode == "navigation_neighbor_failure") {
            foreach (string stage in new string[] { "navigation_terrain", "navigation_input", "navigation_generation", "navigation_validation", "navigation_probe" }) Write(Unavailable(stage));
        } else {
            Write(Record("navigation_terrain").Replace("\"metrics\":{}", "\"metrics\":{\"navigation_tiles\":3}"));
            Write(Record("navigation_input").Replace("\"metrics\":{}", "\"metrics\":{\"solid_vertices\":3,\"solid_triangles\":1,\"liquid_vertices\":4,\"liquid_triangles\":2,\"request_bytes\":512}").Replace("\"hashes\":{}", "\"hashes\":{\"navigation_input\":\"" + new String('d',64) + "\"}"));
            if (mode == "navigation_helper_failure") {
                Write(Record("navigation_generation").Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"navigation.helper_missing\""));
                Write(Unavailable("navigation_validation")); Write(Unavailable("navigation_probe"));
            } else if (mode == "navigation_empty" || mode == "navigation_exact625") {
                Write(NavigationGeneration(mode == "navigation_empty" ? 1 : 2));
                Write(Unavailable("navigation_validation").Replace("collision.prerequisite", mode == "navigation_empty" ? "navigation.empty" : "navigation.exact625")); Write(Unavailable("navigation_probe"));
            } else {
                Write(NavigationGeneration(0)); Write(NavigationValidation());
                for (int i = 0; i < 9; i++) {
                    if (mode == "navigation_load_failure") { Write(Record("navigation_probe").Replace("\"passed\"", "\"failed\"").Replace("\"code\":null", "\"code\":\"navigation.native_format\"")); break; }
                    if (mode == "navigation_partial" && i != 0 && i != 8) continue;
                    Write(mode == "navigation_miss" && i == 0 ? NavigationMiss(i) : mode == "navigation_error" && i == 0 ? NavigationError(i) : NavigationProbe(i));
                }
            }
        }
        Write(mode == "navigation_neighbor_failure" ? Unavailable("navigation_freshness") : Record("navigation_freshness")); Write(Record("freshness"));
        int completedNavigation = mode == "navigation_partial" ? 2 : mode == "navigation_error" ? 8 : mode == "navigation_helper_failure" || mode == "navigation_neighbor_failure" || mode == "navigation_empty" || mode == "navigation_exact625" || mode == "navigation_load_failure" ? 0 : 9;
        int failedStages = mode == "navigation_helper_failure" || mode == "navigation_neighbor_failure" || mode == "navigation_error" || mode == "navigation_load_failure" ? 1 : 0;
        string complete = Record("complete").Replace("\"metrics\":{}", "\"metrics\":{\"completed_probes\":9,\"completed_static_probes\":9,\"completed_navigation_probes\":" + completedNavigation + ",\"failed_stages\":" + failedStages + "}");
        if (failedStages != 0 || completedNavigation != 9) complete = complete.Replace("\"passed\"", "\"failed\"");
        Write(complete); return 0;
    }
}
'@
    $playerSourcePath = Join-Path $root 'AuthoredFakePlayer.cs'; $playerExe = Join-Path $root 'benilla.exe'
    [IO.File]::WriteAllText($playerSourcePath, $playerSource)
    $compilerOutput = & $compiler /nologo /target:exe ("/out:" + $playerExe) $playerSourcePath 2>&1
    Assert-True ($LASTEXITCODE -eq 0 -and [IO.File]::Exists($playerExe)) ('Authored player fixture compilation failed: ' + ($compilerOutput -join ' '))
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
        $reports.Add($output)
        $raw = [IO.File]::ReadAllText($output)
        Assert-True ($raw -notmatch 'PRIVATE_|Private Person|Benilla private path|[A-Za-z]:\\') ($Mode + ': private text leaked into report')
        Assert-ReportSerialization $output $raw $Mode
        $r = ConvertFrom-Json -InputObject $raw
        Assert-True ($r.helper.sha256 -eq (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()) ($Mode + ': helper hash mismatch')
        return $r
    }
    $r = Run-Case 'diagnose'
    Assert-True ($null -eq $r.PSObject.Properties['player'] -and $null -eq $r.PSObject.Properties['collision_run']) 'No -Player: report schema changed'
    Assert-True ($r.helper_run.mode -eq '--diagnose' -and $r.helper_run.exit_code -eq 0) 'diagnose: wrong execution mode'
    Assert-True ($r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'diagnose: schema not recognized'
    Assert-True ($r.detailed_diagnostics.report.map_table.selected_row_ordinal -eq 2) 'diagnose: lost selected map row'
    Assert-True ($r.detailed_diagnostics.report.tiles[0].stages.liquid.diagnostic_id -eq 'terrain.liquid_history') 'diagnose: lost safe diagnostic ID'
    Assert-True ($r.detailed_diagnostics.report.tiles[0].history.consulted_adts -eq 2 -and $r.detailed_diagnostics.report.tiles[0].stages.history_resolution.status -eq 'passed') 'diagnose: lost bounded history observations'
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
    function Run-CollisionCase([string]$Mode, [string]$TerrainMode = 'diagnose', [int]$Timeout = 8, [int]$SelectedMap = 0, [string]$SelectedTile = '48,32', [switch]$SkipTerrain, [string]$PlayerPath = '.\benilla.exe') {
        [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', $TerrainMode, 'Process')
        [Environment]::SetEnvironmentVariable('BENILLA_COLLISION_TEST_MODE', $Mode, 'Process')
        $output = Join-Path $root ('collision-' + $reports.Count + '.json')
        $messages = & $Collector -Helper '.\benilla-native-terrain.exe' -Player $PlayerPath -Output $output -TimeoutSeconds $Timeout -Map $SelectedMap -Tile $SelectedTile -SkipHelper:$SkipTerrain
        Assert-True ([IO.File]::Exists($output)) ($Mode + ': collision report not created')
        $reports.Add($output)
        $raw = [IO.File]::ReadAllText($output)
        Assert-True ($raw -notmatch 'PRIVATE_|Private Person|Benilla private path|[A-Za-z]:\\') ($Mode + ': collision private text leaked')
        Assert-ReportSerialization $output $raw ('collision ' + $Mode)
        $r = ConvertFrom-Json -InputObject $raw
        if ($r.player.status -eq 'available') {
            Assert-True ($r.player.sha256 -eq (Get-FileHash -LiteralPath $playerExe -Algorithm SHA256).Hash.ToLowerInvariant() -and $r.player.unchanged_after_collection) ($Mode + ': player identity/hash mismatch')
        }
        Assert-True ($null -eq $r.collision_run.PSObject.Properties['stdout'] -and $null -eq $r.collision_run.PSObject.Properties['stderr']) ($Mode + ': raw player output retained')
        Assert-True ($r.collision_diagnostics.records.Count -le 64) ($Mode + ': record count limit exceeded')
        if ($null -ne $r.collision_diagnostics.PSObject.Properties['accepted_bytes']) {
            Assert-True ($r.collision_diagnostics.accepted_bytes -le 262144) ($Mode + ': cumulative JSON byte limit exceeded')
            # Canonical records contain only allowlisted ASCII strings and bounded
            # integers. Compact serialization must not expand their NDJSON budget.
            $utf8 = New-Object Text.UTF8Encoding($false)
            $retainedBytes = 0
            foreach ($record in $r.collision_diagnostics.records) {
                $recordJson = ConvertTo-Json -InputObject $record -Depth 16 -Compress
                $retainedBytes += $utf8.GetByteCount($recordJson) + 1
            }
            Assert-True ($retainedBytes -le $r.collision_diagnostics.accepted_bytes) ($Mode + ': compact records exceed accepted NDJSON bytes')
        }
        return $r
    }
    $r = Run-CollisionCase 'success'
    Assert-True ($r.collision_run.status -eq 'completed' -and $r.collision_run.exit_code -eq 0) 'collision success: player did not complete'
    Assert-True ($r.collision_diagnostics.status -eq 'recognized_records' -and $r.collision_diagnostics.records.Count -eq 46) 'collision success: records not preserved'
    Assert-True ($r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'collision success: terrain results lost'
    Assert-True ($r.collision_diagnostics.records[2].metrics.read_work_bytes -eq [long]::MaxValue) 'collision success: Int64 maximum rejected'
    Assert-True ($r.collision_diagnostics.records[4].metrics.tile_bytes -eq 64 -and $r.collision_diagnostics.records[4].metrics.serialized_bytes -eq 224 -and $r.collision_diagnostics.records[4].metrics.tree_bytes -eq 32 -and $r.collision_diagnostics.records[4].metrics.model_bytes -eq 128) 'collision assembly: serialized sizing evidence lost'
    Assert-True ($r.collision_diagnostics.records[6].probe.from_bits[2] -eq 4294967295 -and $null -eq $r.collision_diagnostics.records[6].probe.to_bits) 'collision success: height bits or null changed'
    Assert-True ($r.collision_diagnostics.records[7].probe.to_bits.Count -eq 3 -and $r.collision_diagnostics.records[7].probe.value_bits -eq 4294967295) 'collision success: segment bits changed'
    Assert-True ($r.collision_diagnostics.records[1].stage -eq 'terrain_capture' -and $r.collision_diagnostics.records[1].metrics.terrain_tiles -eq 1 -and $r.collision_diagnostics.records[1].metrics.terrain_bytes -eq 1234 -and $r.collision_diagnostics.records[1].hashes.terrain_source_epoch -eq ('b' * 64)) 'terrain capture: bounded evidence lost'
    Assert-True ($r.collision_diagnostics.records[15].stage -eq 'composition' -and $r.collision_diagnostics.records[15].hashes.composition -eq ('c' * 64)) 'composition: binding evidence lost'
    for ($i = 0; $i -lt 9; $i++) {
        $static = $r.collision_diagnostics.records[16 + $i]
        Assert-True ($static.stage -eq 'static_probe' -and $static.probe.index -eq $i -and $static.probe.kind -eq 'static_height' -and $null -eq $static.probe.to_bits -and $static.probe.from_bits[2] -eq 4294967295) 'composition: nine independent static probes not preserved'
    }
    Assert-True ($r.collision_diagnostics.records[25].stage -eq 'composition_freshness' -and $r.collision_diagnostics.records[44].stage -eq 'freshness' -and $r.collision_diagnostics.records[45].metrics.completed_static_probes -eq 9) 'composition: freshness/completion evidence lost'
    for ($i = 0; $i -lt 4; $i++) {
        $neighbor = $r.collision_diagnostics.records[26 + $i]
        Assert-True ($neighbor.stage -eq ('navigation_neighbor_' + $i) -and $neighbor.status -eq 'passed' -and $neighbor.metrics.neighbor_index -eq $i -and $neighbor.metrics.neighbor_present -eq ($i % 2)) 'navigation: absent/present neighbor observations changed'
    }
    Assert-True ($r.collision_diagnostics.records[30].metrics.navigation_tiles -eq 3 -and $r.collision_diagnostics.records[31].metrics.solid_vertices -eq 3 -and $r.collision_diagnostics.records[31].metrics.solid_triangles -eq 1 -and $r.collision_diagnostics.records[31].metrics.liquid_vertices -eq 4 -and $r.collision_diagnostics.records[31].metrics.liquid_triangles -eq 2 -and $r.collision_diagnostics.records[31].metrics.request_bytes -eq 512 -and $r.collision_diagnostics.records[31].hashes.navigation_input -eq ('d' * 64)) 'navigation: combined terrain/input evidence lost'
    Assert-True ($r.collision_diagnostics.records[32].metrics.native_peak_bytes -eq 65536 -and $r.collision_diagnostics.records[32].metrics.navigation_output_bytes -eq 456 -and $r.collision_diagnostics.records[32].metrics.navigation_polygons -eq 42 -and $r.collision_diagnostics.records[32].metrics.navigation_status -eq 0 -and $r.collision_diagnostics.records[33].hashes.navigation_output -eq ('e' * 64)) 'navigation: validated generation summary/hash lost'
    for ($i = 0; $i -lt 9; $i++) {
        $navigation = $r.collision_diagnostics.records[34 + $i]
        Assert-True ($navigation.stage -eq 'navigation_probe' -and $navigation.probe.index -eq $i -and $navigation.probe.kind -eq 'nearest' -and $navigation.probe.result -eq 'nearest' -and $navigation.probe.from_bits[2] -eq 4294967295 -and $navigation.probe.to_bits[0] -eq 4294967295 -and $null -eq $navigation.probe.value_bits -and $navigation.metrics.polygon_ref_low -eq 4294967295 -and $navigation.metrics.polygon_ref_high -eq 0) 'navigation: nine independently indexed nearest observations changed'
    }
    Assert-True ($r.collision_diagnostics.records[43].stage -eq 'navigation_freshness' -and $r.collision_diagnostics.records[45].metrics.completed_navigation_probes -eq 9 -and -not $r.collision_diagnostics.records[45].gameplay_admitted) 'navigation: freshness, completion or no-admission evidence lost'
    $passed++; Write-Output 'PASS: optional player, all 46 v3 records, bounded numeric/hash/probe schema and Unicode executable/Data paths'
    $r = Run-CollisionCase 'navigation_miss'
    Assert-True ($r.collision_diagnostics.status -eq 'recognized_records' -and $r.collision_diagnostics.records.Count -eq 46 -and $r.collision_diagnostics.records[34].status -eq 'passed' -and $r.collision_diagnostics.records[34].probe.result -eq 'miss' -and $null -eq $r.collision_diagnostics.records[34].probe.to_bits -and $r.collision_diagnostics.records[45].metrics.completed_navigation_probes -eq 9) 'navigation miss: completed absence observation changed'
    $r = Run-CollisionCase 'navigation_error'
    Assert-True ($r.collision_diagnostics.status -eq 'recognized_records' -and $r.collision_diagnostics.records.Count -eq 46 -and $r.collision_diagnostics.records[34].status -eq 'failed' -and $r.collision_diagnostics.records[34].probe.result -eq 'error' -and $r.collision_diagnostics.records[34].code -eq 'navigation.native_query' -and $null -eq $r.collision_diagnostics.records[34].probe.to_bits -and $r.collision_diagnostics.records[45].metrics.completed_navigation_probes -eq 8) 'navigation error: indexed query failure changed'
    $r = Run-CollisionCase 'navigation_partial'
    Assert-True ($r.collision_diagnostics.status -eq 'recognized_records' -and $r.collision_diagnostics.records.Count -eq 39 -and $r.collision_diagnostics.records[34].probe.index -eq 0 -and $r.collision_diagnostics.records[35].probe.index -eq 8 -and $r.collision_diagnostics.records[38].metrics.completed_navigation_probes -eq 2) 'navigation: sparse eligible sample indices were renumbered or manufactured'
    foreach ($mode in @('navigation_empty','navigation_exact625','navigation_helper_failure','navigation_neighbor_failure','navigation_load_failure')) {
        $r = Run-CollisionCase $mode
        Assert-True ($r.collision_diagnostics.status -eq 'recognized_records' -and $r.collision_diagnostics.records.Count -eq 38 -and $r.collision_diagnostics.records[25].stage -eq 'composition_freshness' -and $r.collision_diagnostics.records[37].metrics.completed_navigation_probes -eq 0) ($mode + ': independent terrain/collision/static evidence lost')
        Assert-True ($r.collision_diagnostics.records[34].stage -eq 'navigation_probe' -and $null -eq $r.collision_diagnostics.records[34].probe) ($mode + ': synthetic query evidence manufactured')
        if ($mode -eq 'navigation_helper_failure') {
            Assert-True ($r.collision_diagnostics.records[31].metrics.request_bytes -eq 512 -and $r.collision_diagnostics.records[32].code -eq 'navigation.helper_missing' -and @($r.collision_diagnostics.records[32].metrics.PSObject.Properties).Count -eq 0) 'navigation helper failure: request evidence lost or output metrics fabricated'
        } elseif ($mode -eq 'navigation_neighbor_failure') {
            Assert-True ($r.collision_diagnostics.records[27].status -eq 'failed' -and $r.collision_diagnostics.records[27].code -eq 'source.open_patch_chain' -and $r.collision_diagnostics.records[30].status -eq 'unavailable') 'navigation neighbor failure: genuine source failure treated as absent'
        } elseif ($mode -eq 'navigation_load_failure') {
            Assert-True ($r.collision_diagnostics.records[34].status -eq 'failed' -and $r.collision_diagnostics.records[34].code -eq 'navigation.native_format') 'navigation load failure: safe unindexed loader error lost'
        } else {
            $status = $(if ($mode -eq 'navigation_empty') { 1 } else { 2 })
            $code = $(if ($mode -eq 'navigation_empty') { 'navigation.empty' } else { 'navigation.exact625' })
            Assert-True ($r.collision_diagnostics.records[32].status -eq 'passed' -and $r.collision_diagnostics.records[32].metrics.navigation_status -eq $status -and $r.collision_diagnostics.records[32].metrics.navigation_output_bytes -eq 0 -and $r.collision_diagnostics.records[33].status -eq 'unavailable' -and $r.collision_diagnostics.records[33].code -eq $code) ($mode + ': terminal generation outcome lost')
        }
    }
    $passed++; Write-Output 'PASS: navigation absence, nearest misses, indexed errors, sparse probes, helper failures and empty/exact625 outcomes'
    $r = Run-CollisionCase 'success' 'nonzero'
    Assert-True ($r.helper_run.exit_code -eq 7 -and $r.collision_diagnostics.records.Count -eq 46) 'terrain failure prevented collision success'
    $r = Run-CollisionCase 'failure'
    Assert-True ($r.detailed_diagnostics.status -eq 'recognized_schema_filtered' -and $r.collision_run.exit_code -eq 7) 'collision failure lost independent terrain success'
    Assert-True ($r.collision_diagnostics.records.Count -eq 22 -and $r.collision_diagnostics.records[2].code -eq 'vmap.source.member_read' -and $r.collision_diagnostics.records[2].source_stage -eq 'AdtTraversal') 'collision failure lost safe source diagnostic'
    Assert-True ($r.collision_diagnostics.records[6].status -eq 'unavailable' -and $null -eq $r.collision_diagnostics.records[6].probe) 'collision failure lost unavailable probe stage'
    Assert-True ($r.collision_diagnostics.records[1].stage -eq 'terrain_capture' -and $r.collision_diagnostics.records[1].status -eq 'passed' -and $r.collision_diagnostics.records[7].stage -eq 'composition' -and $r.collision_diagnostics.records[8].stage -eq 'static_probe' -and $r.collision_diagnostics.records[8].status -eq 'unavailable') 'collision failure lost independent player terrain capture or blocked composition'
    $r = Run-CollisionCase 'success' -SkipTerrain
    Assert-True ($r.helper_run.status -eq 'skipped_by_request' -and $r.collision_diagnostics.records.Count -eq 46) 'SkipHelper incorrectly skipped collision'
    $r = Run-CollisionCase 'success' -PlayerPath '.\missing\benilla.exe'
    Assert-True ($r.player.status -eq 'missing' -and $r.collision_run.status -eq 'skipped_player_unavailable' -and $r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'missing player blocked terrain diagnostics'
    $passed++; Write-Output 'PASS: independent terrain/collision failures, missing player and terrain skip'
    $r = Run-CollisionCase 'timeout' -Timeout 1
    Assert-True ($r.collision_run.timed_out -and $r.collision_run.termination_confirmed -and $r.collision_run.elapsed_ms -ge 6000 -and $r.collision_run.elapsed_ms -lt 10000) 'collision timeout: five-second grace or bounded hard termination missing'
    Assert-True ($r.collision_diagnostics.records.Count -eq 3 -and $r.collision_diagnostics.partial_line_omitted) 'collision timeout: complete partial records lost or trailing text retained'
    Assert-True ($r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'collision timeout: terrain result lost'
    $r = Run-CollisionCase 'soft_timeout' -Timeout 1
    Assert-True ($r.collision_run.status -eq 'completed' -and -not $r.collision_run.timed_out -and $r.collision_run.exit_code -eq 7 -and $r.collision_diagnostics.records.Count -eq 4 -and $r.collision_diagnostics.records[2].code -eq 'collision.timeout') 'collision soft timeout: final cooperative diagnostics lost before hard grace deadline'
    $r = Run-CollisionCase 'success' 'timeout' 1
    Assert-True ($r.helper_run.timed_out -and $r.collision_diagnostics.records.Count -eq 46) 'terrain timeout prevented collision collection'
    $passed++; Write-Output 'PASS: hard timeout retains complete NDJSON records and independent terrain results'
    $r = Run-CollisionCase 'flood'
    Assert-True ($r.collision_run.stdout_truncated -and $r.collision_run.stderr_truncated -and $r.collision_run.status -eq 'output_limit_exceeded' -and $r.collision_run.elapsed_ms -lt 8000) 'collision flood: streams not bounded or drain deadlocked'
    Assert-True ($r.collision_diagnostics.records.Count -eq 2) 'collision flood: earlier valid record lost'
    $r = Run-CollisionCase 'oversized'
    Assert-True ($r.collision_diagnostics.records.Count -eq 3 -and $r.collision_diagnostics.oversized_lines_omitted -eq 1) 'collision oversized: safe adjacent records not preserved'
    $r = Run-CollisionCase 'malformed'
    Assert-True ($r.collision_diagnostics.records.Count -eq 3 -and $r.collision_diagnostics.invalid_records_omitted -eq 1) 'collision malformed: safe adjacent records not preserved'
    $passed++; Write-Output 'PASS: stream saturation, oversized lines and malformed later output preserve valid records'
    $r = Run-CollisionCase 'invalid_schema'
    Assert-True ($r.collision_diagnostics.records.Count -eq 3 -and $r.collision_diagnostics.invalid_records_omitted -eq 46) 'collision invalid schema: invalid keys/types/ranges/syntax or private text accepted'
    $r = Run-CollisionCase 'legacy_version'
    Assert-True ($r.collision_diagnostics.records.Count -eq 0 -and $r.collision_diagnostics.invalid_records_omitted -eq 2) 'collision version: v1 records accepted as v3'
    $r = Run-CollisionCase 'legacy_v2'
    Assert-True ($r.collision_diagnostics.records.Count -eq 0 -and $r.collision_diagnostics.invalid_records_omitted -eq 2) 'collision version: v2 records accepted as v3'
    $r = Run-CollisionCase 'mixed_version'
    Assert-True ($r.collision_diagnostics.status -eq 'invalid_or_excess_records_omitted' -and $r.collision_diagnostics.records.Count -eq 3 -and $r.collision_diagnostics.invalid_records_omitted -eq 2) 'collision version: mixed versions were accepted as a valid v3 report'
    $r = Run-CollisionCase 'invalid_static_schema'
    Assert-True ($r.collision_diagnostics.records.Count -eq 6 -and $r.collision_diagnostics.invalid_records_omitted -eq 12 -and $r.collision_diagnostics.records[5].probe.index -eq 8) 'static schema: invalid kinds/types/ranges or private text accepted'
    $r = Run-CollisionCase 'invalid_navigation_schema'
    Assert-True ($r.collision_diagnostics.records.Count -eq 6 -and $r.collision_diagnostics.invalid_records_omitted -eq 37 -and $r.collision_diagnostics.records[3].probe.result -eq 'nearest' -and $r.collision_diagnostics.records[4].probe.result -eq 'miss' -and $r.collision_diagnostics.records[5].probe.result -eq 'error') 'navigation schema: invalid status/kinds/types/ranges/hashes or private text accepted'
    $r = Run-CollisionCase 'order'
    Assert-True ($r.collision_diagnostics.records.Count -eq 11 -and $r.collision_diagnostics.invalid_records_omitted -eq 7) 'collision order: duplicate phase/probe or out-of-order record accepted'
    $r = Run-CollisionCase 'budget'
    Assert-True ($r.collision_diagnostics.records.Count -gt 1 -and $r.collision_diagnostics.limit_records_omitted -gt 0 -and $r.collision_diagnostics.accepted_bytes -le 262144) 'collision budget: accepted JSON unbounded or prior records lost'
    $r = Run-CollisionCase 'newline_budget'
    Assert-True ($r.collision_diagnostics.records.Count -eq 31 -and $r.collision_diagnostics.limit_records_omitted -eq 1 -and $r.collision_diagnostics.accepted_bytes -eq (31 * 8193)) 'collision budget: NDJSON newline bytes not counted'
    $r = Run-CollisionCase 'record_limit'
    Assert-True ($r.collision_diagnostics.records.Count -eq 64 -and $r.collision_diagnostics.limit_records_omitted -eq 10) 'collision record count: 64-record cap not enforced'
    $passed++; Write-Output 'PASS: exact schema, duplicate keys, numeric types/ranges, stage order and cumulative limits'
    $r = Run-CollisionCase 'source_codes'
    Assert-True ($r.collision_diagnostics.records.Count -eq 8 -and $r.collision_diagnostics.records[2].code -eq 'vmap.source.header_version' -and $r.collision_diagnostics.records[3].code -eq 'vmap.source_changed_during_preparation' -and $r.collision_diagnostics.records[4].code -eq 'vmap.bih_limit' -and $r.collision_diagnostics.records[5].code -eq 'vmap.model_write.allocation' -and $r.collision_diagnostics.records[6].code -eq 'terrain.source_changed' -and $r.collision_diagnostics.records[7].code -eq 'collision.unknown_error') 'collision source codes: safe codes lost'
    $r = Run-CollisionCase 'success' -SelectedMap 1 -SelectedTile '1,2'
    Assert-True ($r.collision_diagnostics.records.Count -eq 46 -and $r.collision_diagnostics.records[0].map -eq 1 -and $r.collision_diagnostics.records[0].tile_x -eq 1 -and $r.collision_diagnostics.records[0].tile_y -eq 2) 'collision selection: explicit selection not forwarded/matched'
    $passed++; Write-Output 'PASS: fixed source codes, strict requested-selection matching and explicit CLI arguments'
    $r = Run-Case 'diagnose-again'
    Assert-True ($null -eq $r.PSObject.Properties['player'] -and $null -eq $r.PSObject.Properties['collision_run'] -and $r.detailed_diagnostics.status -eq 'recognized_schema_filtered') 'No -Player after collision: legacy report changed'
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
    Assert-True ($newFiles.Count -eq $reports.Count -and @($newFiles | Where-Object { $_.Extension -ne '.json' }).Count -eq 0) 'Collector created files other than its explicit JSON reports'
    $passed++; Write-Output 'PASS: input hashes unchanged and no extra files created'
    Write-Output ('PASS: ' + $passed + ' authored smoke groups under PowerShell ' + $PSVersionTable.PSVersion.ToString())
} finally {
    if (Get-Variable -Name privateLock -ErrorAction SilentlyContinue) { if ($null -ne $privateLock) { $privateLock.Dispose() } }
    if (Get-Variable -Name mpqLock -ErrorAction SilentlyContinue) { if ($null -ne $mpqLock) { $mpqLock.Dispose() } }
    Set-Location -LiteralPath $oldLocation.Path
    [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_MODE', $oldMode, 'Process')
    [Environment]::SetEnvironmentVariable('BENILLA_DIAG_TEST_DATA', $oldData, 'Process')
    [Environment]::SetEnvironmentVariable('BENILLA_COLLISION_TEST_MODE', $oldCollisionMode, 'Process')
    if ([IO.Directory]::Exists($root)) { Remove-Item -LiteralPath $root -Recurse -Force }
}
