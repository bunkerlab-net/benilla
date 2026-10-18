#requires -Version 5.1
<#
.SYNOPSIS
Collect bounded, path-redacted diagnostics for one Benilla terrain selection.
.DESCRIPTION
Reads the selected local Data folder and existing helper. Creates exactly one NEW
JSON report; never overwrites it. No downloads, network requests, administrator
rights, policy changes, archive extraction, game configuration reads or exports.
The helper is invoked only with --help and --diagnose (or older --inspect).
Use only your trusted benilla-native-terrain.exe: this script is not a sandbox for
an arbitrary executable. Windows itself may maintain normal process/file metadata.

The report includes helper SHA-256, numeric executable version/architecture,
bounded relevant MPQ layout, five archive envelopes, and a single selected-tile
result. It does NOT hash whole archives, dump archive members, scan all maps,
prove gameplay readiness, or guarantee every possible cause can be diagnosed.
Unrecognized helper text is omitted rather than copied into the report.

Default selection: map 0, tile 48,32. --diagnose exit 0 means a diagnostic report
was emitted; check its stages for validation success. Older --inspect can stop
at the first error and cannot expose deep liquid-history diagnostics.

.EXAMPLE
.\Collect-BenillaTerrainDiagnostics.ps1 -Data '.\Data' -Helper '.\benilla-native-terrain.exe' -Output '.\Benilla-Terrain-Diagnostics.json'
.EXAMPLE
.\Collect-BenillaTerrainDiagnostics.ps1 -Data 'E:\Blizzard\Benilla\Data' -Helper 'E:\Tools\benilla-native-terrain.exe' -Output '.\Benilla-Terrain-Diagnostics.json'
.NOTES
Windows PowerShell 5.1 or PowerShell 7 on Windows. Report parent must exist;
report must be outside Data. Reparse-point paths and network drives are refused.
If your execution policy blocks this script, inspect it and use your normal
approved script-running process; the collector does not change that policy.
#>
[CmdletBinding()]
param(
    [string]$Data = '.\Data',
    [Parameter(Mandatory = $true)][string]$Helper,
    [string]$Output = ('.\Benilla-Terrain-Diagnostics-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '.json'),
    [ValidateRange(0, 999)][int]$Map = 0,
    [ValidatePattern('^\d{1,2},\d{1,2}$')][string]$Tile = '48,32',
    [ValidateRange(1, 600)][int]$TimeoutSeconds = 120,
    [switch]$SkipHelper
)
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

# Exact public IDs only: arbitrary dotted helper strings must not enter the report.
$SafeDiagnosticIds = @(
    'terrain.history_map_metadata',
    'terrain.liquid_history_interior',
    'terrain.liquid_history_selection_changed',
    'source.history_lookup_limit',
    'source.history_initial_recheck',
    'source.history_open',
    'source.history_member_profile',
    'source.history_resolver_mismatch',
    'source.history_member_kind',
    'source.absence_member_profile',
    'source.absence_changed',
    'source.ad_absence_profile',
    'source.ad_absence_changed',
    'source.discovery_absence_changed',
    'source.recheck_absence',
    'output.already_exists', 'output.changed_product', 'output.create_new_directory',
    'output.directory_name', 'output.inside_source_installation', 'output.parent_must_exist',
    'output.published_but_durability_unconfirmed', 'selection.bounds', 'selection.duplicate_tile',
    'selection.tile_bounds', 'source.ad_archive_case_ambiguity', 'source.ad_archive_metadata',
    'source.ad_archive_size', 'source.ad_bytes_changed', 'source.ad_deleted',
    'source.ad_directory_entry_limit', 'source.ad_dummy_member', 'source.ad_hash_count',
    'source.ad_header_extension', 'source.ad_header_open', 'source.ad_header_read',
    'source.ad_header_scan_limit', 'source.ad_header_selection', 'source.ad_header_signature',
    'source.ad_header_truncated', 'source.ad_header_version', 'source.ad_index_budget',
    'source.ad_index_spans', 'source.ad_member_profile', 'source.ad_missing_member',
    'source.ad_missing_size', 'source.ad_open_archive', 'source.ad_read',
    'source.ad_regular_archive_required', 'source.ad_sector_size', 'source.ad_winner_mismatch',
    'source.adt_count_limit', 'source.aggregate_byte_limit', 'source.archive_basename',
    'source.bytes_changed', 'source.data_directory', 'source.data_directory_required',
    'source.discovery_changed', 'source.discovery_directory_changed', 'source.discovery_missing',
    'source.duplicate_member', 'source.installation_parent', 'source.map_directory',
    'source.member_byte_limit', 'source.member_count_limit', 'source.member_length_changed',
    'source.member_name_limit', 'source.missing_or_deleted', 'source.missing_size',
    'source.multiple_maps', 'source.not_captured', 'source.open_patch_chain',
    'source.read', 'source.recheck_ad_profile', 'source.recheck_members',
    'source.recheck_open', 'source.recheck_read', 'source.tile_coordinate',
    'source.unselected_member_kind', 'source.wdt_count_limit', 'source.winner_changed',
    'terrain.AreaTable', 'terrain.LiquidType', 'terrain.Map',
    'terrain.adt_mhdr', 'terrain.adt_required_records', 'terrain.adt_unsupported_or_liquid',
    'terrain.adt_version', 'terrain.canonical_mcin_required', 'terrain.cell_count',
    'terrain.chunk_count', 'terrain.chunk_framing', 'terrain.chunk_header',
    'terrain.chunk_overflow', 'terrain.chunk_payload', 'terrain.dangling_root_chunk',
    'terrain.dangling_subchunk', 'terrain.dbc_framing', 'terrain.dbc_id_budget',
    'terrain.dbc_layout', 'terrain.dbc_magic', 'terrain.dbc_nul',
    'terrain.dbc_rows', 'terrain.dbc_size', 'terrain.dbc_string_nul',
    'terrain.dbc_string_offset', 'terrain.dbc_string_utf8', 'terrain.dbc_strings',
    'terrain.duplicate_chunk', 'terrain.duplicate_map_id', 'terrain.duplicate_mcin_reference',
    'terrain.duplicate_subchunk', 'terrain.height_overflow', 'terrain.liquid_empty_bounds',
    'terrain.liquid_excluded', 'terrain.liquid_history', 'terrain.liquid_nonfinite',
    'terrain.map_directory', 'terrain.map_missing', 'terrain.mccv_excluded',
    'terrain.mcin_not_mcnk', 'terrain.mcin_size', 'terrain.mclq_length',
    'terrain.mclq_size', 'terrain.mcnk_header', 'terrain.mcnk_unsupported_or_liquid',
    'terrain.mcvt_length', 'terrain.member_budget', 'terrain.mfbo_excluded',
    'terrain.mh2o_excluded', 'terrain.mhdr_reference', 'terrain.nonfinite_height',
    'terrain.normal_padding', 'terrain.offset_overflow', 'terrain.selected_map_metadata',
    'terrain.subchunk_reference', 'terrain.tile_coordinate', 'terrain.truncated_half',
    'terrain.truncated_word', 'terrain.unselected_map_metadata', 'terrain.wdt_global_or_header',
    'terrain.wdt_global_wmo', 'terrain.wdt_main', 'terrain.wdt_required_records',
    'terrain.wdt_tile_absent', 'terrain.wdt_unsupported_record', 'terrain.wdt_version',
    'terrain.writer_framing', 'terrain.operation_failed', 'cli.mode_conflict',
    'cli.missing_argument', 'cli.invalid_value', 'cli.unknown_argument',
    'cli.invalid_arguments', 'cli.report_failed'
)

function Get-LocalFullPath([string]$Value) {
    $path = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Value)
    if ($path -notmatch '^[A-Za-z]:[\\/]' -or $path.StartsWith('\\')) { throw 'input.local_drive_required' }
    $path = [IO.Path]::GetFullPath($path)
    if ($path.Substring(2).Contains(':')) { throw 'input.alternate_stream_refused' }
    $drive = New-Object IO.DriveInfo([IO.Path]::GetPathRoot($path))
    if ($drive.DriveType -eq [IO.DriveType]::Network) { throw 'input.network_drive_refused' }
    return $path
}
function Test-ReparseAncestor([string]$Path) {
    $current = $Path
    while ($current) {
        if ([IO.File]::Exists($current) -or [IO.Directory]::Exists($current)) {
            if (([IO.File]::GetAttributes($current) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $true }
        }
        $parent = [IO.Path]::GetDirectoryName($current.TrimEnd('\', '/'))
        if ($parent -eq $current) { break }
        $current = $parent
    }
    return $false
}
function Test-InDirectory([string]$Candidate, [string]$Directory) {
    $root = $Directory.TrimEnd('\', '/')
    return $Candidate.Equals($root, [StringComparison]::OrdinalIgnoreCase) -or
        $Candidate.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)
}
function Read-Exact([IO.Stream]$Stream, [byte[]]$Buffer) {
    $read = 0
    while ($read -lt $Buffer.Length) {
        $count = $Stream.Read($Buffer, $read, $Buffer.Length - $read)
        if ($count -eq 0) { throw 'read.incomplete' }
        $read += $count
    }
}
function Get-Sha256([string]$Path) {
    $stream = $null; $hash = $null
    try {
        $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        if ($stream.Length -gt 536870912) { throw 'helper.hash_size_limit' }
        $hash = [Security.Cryptography.SHA256]::Create()
        return ([BitConverter]::ToString($hash.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    } finally {
        if ($null -ne $hash) { $hash.Dispose() }
        if ($null -ne $stream) { $stream.Dispose() }
    }
}
function Get-HelperIdentity([string]$Path) {
    $result = [ordered]@{ status = 'unavailable'; basename = 'benilla-native-terrain.exe'; bytes = $null;
        sha256 = $null; sha256_after_collection = $null; unchanged_after_collection = $null;
        version = $null; pe_machine = $null; pe_machine_name = $null }
    $stream = $null
    try {
        if ([IO.Path]::GetFileName($Path) -ine 'benilla-native-terrain.exe') { $result.status = 'unexpected_basename'; return $result }
        if (-not [IO.File]::Exists($Path)) { $result.status = 'missing'; return $result }
        if (Test-ReparseAncestor $Path) { $result.status = 'reparse_path_refused'; return $result }
        $file = New-Object IO.FileInfo($Path)
        $result.bytes = $file.Length
        if ($file.Length -gt 536870912) { $result.status = 'helper_size_limit'; return $result }
        $result.sha256 = Get-Sha256 $Path
        $v = [Diagnostics.FileVersionInfo]::GetVersionInfo($Path)
        $result.version = [ordered]@{ file_major = $v.FileMajorPart; file_minor = $v.FileMinorPart;
            file_build = $v.FileBuildPart; file_private = $v.FilePrivatePart;
            product_major = $v.ProductMajorPart; product_minor = $v.ProductMinorPart;
            product_build = $v.ProductBuildPart; product_private = $v.ProductPrivatePart }
        $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $dos = New-Object byte[] 64
        Read-Exact $stream $dos
        if ($dos[0] -ne 77 -or $dos[1] -ne 90) { $result.status = 'not_pe_executable'; return $result }
        $offset = [BitConverter]::ToUInt32($dos, 60)
        if ($offset -gt 1048576 -or ([long]$offset + 6) -gt $stream.Length) { $result.status = 'pe_header_out_of_bounds'; return $result }
        [void]$stream.Seek($offset, [IO.SeekOrigin]::Begin)
        $pe = New-Object byte[] 6; Read-Exact $stream $pe
        if ([BitConverter]::ToUInt32($pe, 0) -ne 17744) { $result.status = 'not_pe_executable'; return $result }
        $result.pe_machine = [BitConverter]::ToUInt16($pe, 4)
        $result.pe_machine_name = switch ($result.pe_machine) { 34404 { 'x64' } 332 { 'x86' } 43620 { 'arm64' } default { 'other' } }
        $result.status = 'available'
    } catch { $result.status = 'read_failed' }
    finally { if ($null -ne $stream) { $stream.Dispose() } }
    return $result
}
function Get-ArchiveEnvelope([string]$Directory, [string]$Name, [bool]$Available) {
    $r = [ordered]@{ basename = $Name; status = 'data_unavailable'; file_bytes = $null;
        scan_limit_bytes = 1048576; scan_alignment_bytes = 512; scan_candidates = 0;
        first_magic = $null; header = $null; archive_sha256 = 'skipped_full_archive_hash' }
    if (-not $Available) { return $r }
    $stream = $null
    try {
        $path = [IO.Path]::Combine($Directory, $Name)
        if (-not [IO.File]::Exists($path)) { $r.status = 'absent'; return $r }
        if (([IO.File]::GetAttributes($path) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { $r.status = 'reparse_file_refused'; return $r }
        $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $r.file_bytes = $stream.Length
        if ($stream.Length -ge 4) {
            $magic = New-Object byte[] 4; Read-Exact $stream $magic
            $r.first_magic = [BitConverter]::ToUInt32($magic, 0)
        }
        for ([long]$offset = 0; $offset -lt 1048576 -and ($offset + 4) -le $stream.Length; $offset += 512) {
            [void]$stream.Seek($offset, [IO.SeekOrigin]::Begin)
            $signature = New-Object byte[] 4; Read-Exact $stream $signature
            $r.scan_candidates++
            if ([BitConverter]::ToUInt32($signature, 0) -ne 441536589) { continue }
            if ($offset + 32 -gt $stream.Length) { $r.status = 'header_truncated'; return $r }
            [void]$stream.Seek($offset, [IO.SeekOrigin]::Begin)
            $bytes = New-Object byte[] 32; Read-Exact $stream $bytes
            $h = [ordered]@{ offset = $offset; header_size = [BitConverter]::ToUInt32($bytes, 4);
                archive_bytes = [BitConverter]::ToUInt32($bytes, 8); raw_version = [BitConverter]::ToUInt16($bytes, 12);
                sector_shift = [BitConverter]::ToUInt16($bytes, 14); sector_bytes = $null;
                hash_table_offset_low = [BitConverter]::ToUInt32($bytes, 16);
                block_table_offset_low = [BitConverter]::ToUInt32($bytes, 20);
                hash_entries = [BitConverter]::ToUInt32($bytes, 24); block_entries = [BitConverter]::ToUInt32($bytes, 28);
                extension_status = 'not_applicable'; extended_block_table_offset = $null;
                hash_table_offset_high = $null; block_table_offset_high = $null;
                extended_offsets_all_zero = $null; declared_archive_matches_remaining_file = $null;
                hash_count_nonzero_power_of_two = $null; index_spans_within_archive = $null;
                index_spans_nonoverlapping = $null; index_total_bytes = $null }
            if ($h.sector_shift -le 31) { $h.sector_bytes = [long]512 * [long][Math]::Pow(2, $h.sector_shift) }
            $r.header = $h
            $r.status = 'header_found'
            if ($h.raw_version -eq 1) {
                if ($offset + 44 -gt $stream.Length) { $h.extension_status = 'truncated'; $r.status = 'header_truncated'; return $r }
                $extra = New-Object byte[] 12; Read-Exact $stream $extra
                $h.extended_block_table_offset = [BitConverter]::ToUInt64($extra, 0)
                $h.hash_table_offset_high = [BitConverter]::ToUInt16($extra, 8)
                $h.block_table_offset_high = [BitConverter]::ToUInt16($extra, 10)
                $h.extended_offsets_all_zero = $h.extended_block_table_offset -eq 0 -and $h.hash_table_offset_high -eq 0 -and $h.block_table_offset_high -eq 0
                $h.extension_status = 'read'
            } elseif ($h.raw_version -gt 1) { $h.extension_status = 'unsupported_version_not_decoded' }
            if ($h.raw_version -eq 0 -or ($h.raw_version -eq 1 -and $h.extended_offsets_all_zero)) {
                $h.declared_archive_matches_remaining_file = [long]$h.archive_bytes -eq ($stream.Length - $offset)
                $h.hash_count_nonzero_power_of_two = $h.hash_entries -ne 0 -and ($h.hash_entries -band ($h.hash_entries - 1)) -eq 0
                $hashBytes = [long]$h.hash_entries * 16; $blockBytes = [long]$h.block_entries * 16
                $hashEnd = [long]$h.hash_table_offset_low + $hashBytes; $blockEnd = [long]$h.block_table_offset_low + $blockBytes
                $h.index_total_bytes = $hashBytes + $blockBytes
                $h.index_spans_within_archive = $h.hash_table_offset_low -ge $h.header_size -and $h.block_table_offset_low -ge $h.header_size -and $hashEnd -le $h.archive_bytes -and $blockEnd -le $h.archive_bytes
                $h.index_spans_nonoverlapping = $hashEnd -le $h.block_table_offset_low -or $blockEnd -le $h.hash_table_offset_low
            }
            return $r
        }
        $r.status = 'no_aligned_header_within_limit'
    } catch { $r.status = 'read_failed' }
    finally { if ($null -ne $stream) { $stream.Dispose() } }
    return $r
}
function Get-ArchiveLayout([string]$Directory, [bool]$Available) {
    $r = [ordered]@{ status = 'data_unavailable'; entry_limit = 1024; entries_examined = 0;
        archive_limit = 128; truncated = $false; unrecognized_mpq_names = 0; archives = @(); locale_directories = @() }
    if (-not $Available) { return $r }
    $names = '(?i)^(?:base|dbc|fonts|interface|item|misc|model|sound|speech|terrain|texture|wmo|world|expansion|common(?:-\d+)?|patch(?:-\d+)?|(?:locale|speech|patch)-[a-z]{4}(?:-\d+)?)\.mpq$'
    $locales = @('enUS','enGB','deDE','frFR','esES','esMX','ruRU','koKR','zhCN','zhTW','itIT','ptBR','ptPT')
    $folders = New-Object 'Collections.Generic.List[object]'
    $folders.Add([pscustomobject]@{ path = $Directory; label = 'Data' })
    $archives = New-Object 'Collections.Generic.List[object]'
    try {
        for ($folderIndex = 0; $folderIndex -lt $folders.Count; $folderIndex++) {
            $folder = $folders[$folderIndex]
            foreach ($entry in [IO.Directory]::EnumerateFileSystemEntries($folder.path)) {
                if ($r.entries_examined -ge $r.entry_limit) { $r.truncated = $true; break }
                $r.entries_examined++
                $name = [IO.Path]::GetFileName($entry)
                $attributes = [IO.File]::GetAttributes($entry)
                if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) {
                    if ($folderIndex -eq 0 -and $locales -ccontains $name) {
                        $isLink = ($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
                        $r.locale_directories += [ordered]@{ locale = $name; status = $(if ($isLink) { 'reparse_directory_refused' } else { 'present' }) }
                        if (-not $isLink) { $folders.Add([pscustomobject]@{ path = $entry; label = 'Data/' + $name }) }
                    }
                    continue
                }
                if ([IO.Path]::GetExtension($name) -ine '.mpq') { continue }
                if ($name -notmatch $names) { $r.unrecognized_mpq_names++; continue }
                if ($archives.Count -ge $r.archive_limit) { $r.truncated = $true; continue }
                $isLink = ($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
                $length = $null
                if (-not $isLink) { $length = (New-Object IO.FileInfo($entry)).Length }
                $archives.Add([ordered]@{ location = $folder.label; basename = $name.ToLowerInvariant(); bytes = $length;
                    status = $(if ($isLink) { 'reparse_file_refused' } else { 'present' }) })
            }
            if ($r.entries_examined -ge $r.entry_limit) { break }
        }
        $r.status = $(if ($r.truncated) { 'bounded_partial' } else { 'complete_within_scope' })
    } catch { $r.status = 'read_failed_or_partial' }
    $r.archives = @($archives.ToArray() | Sort-Object { $_.location }, { $_.basename })
    return $r
}
function ConvertTo-WindowsArgument([string]$Value) {
    # CommandLineToArgvW/CRT escaping; works on .NET Framework without ArgumentList.
    $b = New-Object Text.StringBuilder
    [void]$b.Append('"'); $slashes = 0
    foreach ($c in $Value.ToCharArray()) {
        if ($c -eq '\') { $slashes++; continue }
        if ($c -eq '"') { [void]$b.Append(('\' * (2 * $slashes + 1))); [void]$b.Append('"') }
        else { [void]$b.Append(('\' * $slashes)); [void]$b.Append($c) }
        $slashes = 0
    }
    [void]$b.Append(('\' * (2 * $slashes))); [void]$b.Append('"')
    return $b.ToString()
}
function Invoke-BoundedHelper([string]$Executable, [string[]]$Arguments, [int]$Timeout) {
    $r = [ordered]@{ status = 'start_failed'; exit_code = $null; elapsed_ms = 0; timed_out = $false;
        termination_attempted = $false; termination_confirmed = $null;
        stdout_characters_seen = 0; stderr_characters_seen = 0;
        stdout_truncated = $false; stderr_truncated = $false; stdout = ''; stderr = '' }
    $limit = 524288; $totalLimit = 2097152
    $out = New-Object Text.StringBuilder; $err = New-Object Text.StringBuilder
    $process = New-Object Diagnostics.Process; $watch = [Diagnostics.Stopwatch]::StartNew()
    $started = $false; $stopAt = $null
    try {
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = $Executable
        $start.Arguments = (@($Arguments | ForEach-Object { ConvertTo-WindowsArgument $_ }) -join ' ')
        $start.WorkingDirectory = [IO.Path]::GetDirectoryName($Executable)
        $start.UseShellExecute = $false; $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true; $start.RedirectStandardInput = $true
        $start.StandardOutputEncoding = New-Object Text.UTF8Encoding($false)
        $start.StandardErrorEncoding = New-Object Text.UTF8Encoding($false)
        $process.StartInfo = $start
        $started = $process.Start()
        if (-not $started) { return $r }
        $process.StandardInput.Close()
        $r.status = 'completed'
        $outBuffer = New-Object char[] 4096; $errBuffer = New-Object char[] 4096
        $outTask = $process.StandardOutput.ReadAsync($outBuffer, 0, $outBuffer.Length)
        $errTask = $process.StandardError.ReadAsync($errBuffer, 0, $errBuffer.Length)
        $outDone = $false; $errDone = $false
        while ($true) {
            if (-not $outDone -and $outTask.IsCompleted) {
                $count = $outTask.GetAwaiter().GetResult()
                if ($count -eq 0) { $outDone = $true }
                else {
                    $r.stdout_characters_seen += $count
                    $keep = [Math]::Min($count, $limit - $out.Length)
                    if ($keep -gt 0) { [void]$out.Append($outBuffer, 0, $keep) }
                    if ($keep -lt $count) { $r.stdout_truncated = $true }
                    $outTask = $process.StandardOutput.ReadAsync($outBuffer, 0, $outBuffer.Length)
                }
            }
            if (-not $errDone -and $errTask.IsCompleted) {
                $count = $errTask.GetAwaiter().GetResult()
                if ($count -eq 0) { $errDone = $true }
                else {
                    $r.stderr_characters_seen += $count
                    $keep = [Math]::Min($count, $limit - $err.Length)
                    if ($keep -gt 0) { [void]$err.Append($errBuffer, 0, $keep) }
                    if ($keep -lt $count) { $r.stderr_truncated = $true }
                    $errTask = $process.StandardError.ReadAsync($errBuffer, 0, $errBuffer.Length)
                }
            }
            if ($process.HasExited -and $outDone -and $errDone) { break }
            if ($null -eq $stopAt) {
                if ($watch.Elapsed.TotalSeconds -ge $Timeout) { $r.status = 'timed_out'; $r.timed_out = $true; $stopAt = $watch.ElapsedMilliseconds }
                elseif (($r.stdout_characters_seen + $r.stderr_characters_seen) -gt $totalLimit) { $r.status = 'output_limit_exceeded'; $stopAt = $watch.ElapsedMilliseconds }
                if ($null -ne $stopAt -and -not $process.HasExited) {
                    $r.termination_attempted = $true
                    try { $process.Kill() } catch { $r.termination_confirmed = $false }
                }
            }
            if ($null -ne $stopAt -and ($watch.ElapsedMilliseconds - $stopAt) -ge 2000) { break }
            [Threading.Thread]::Sleep(10)
        }
        if ($process.HasExited) { $r.exit_code = $process.ExitCode; if ($r.termination_attempted) { $r.termination_confirmed = $true } }
        elseif ($r.termination_attempted) { $r.termination_confirmed = $false }
    } catch {
        $r.status = $(if ($started) { 'capture_failed' } else { 'start_failed' })
        if ($started) {
            try { if (-not $process.HasExited) { $r.termination_attempted = $true; $process.Kill(); $r.termination_confirmed = $process.WaitForExit(2000) } } catch { $r.termination_confirmed = $false }
        }
    } finally {
        $watch.Stop(); $r.elapsed_ms = $watch.ElapsedMilliseconds
        $r.stdout = $out.ToString(); $r.stderr = $err.ToString()
        $process.Dispose()
    }
    return $r
}
function ConvertTo-SafeText([string]$Text) {
    $lines = New-Object 'Collections.Generic.List[string]'; $omitted = 0
    foreach ($line in ($Text -split '\r?\n')) {
        if ($line.Length -eq 0) { continue }
        if ($lines.Count -ge 128) { $omitted++; continue }
        if ($line -cmatch '^error\[((?:terrain|source|selection|cli|output)\.[A-Za-z0-9_]{1,64})\]:') {
            if ($SafeDiagnosticIds -ccontains $Matches[1]) { $lines.Add('error[' + $Matches[1] + ']: [explanation omitted]') } else { $omitted++ }
        } elseif ($line -cmatch '^(?:Inspected [0-9]{1,2} terrain tiles in memory; no artifacts published|map=[0-9]{1,3} source_epoch=[0-9a-f]{64}|tile=[0-9]{1,2},[0-9]{1,2} bytes=[0-9]{1,12} sha256=[0-9a-f]{64}|live_install_lease=false gameplay_admitted=false|No live installation lease or gameplay admission established)$') {
            $lines.Add($line)
        } else { $omitted++ }
    }
    return [ordered]@{ text = ($lines.ToArray() -join "`n"); unrecognized_lines_omitted = $omitted }
}
function Read-Field($Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}
function Read-SafeInteger($Value, [long]$Maximum) {
    if ($null -eq $Value -or $Value -is [bool] -or $Value -is [string]) { return $null }
    if ($Value -isnot [ValueType]) { return $null }
    try { $n = [decimal]$Value; if ($n -ge 0 -and $n -le $Maximum -and [decimal]::Truncate($n) -eq $n) { return [long]$n } } catch { }
    return $null
}
function Read-SafeHash($Value) { if ($Value -is [string] -and $Value -cmatch '^[0-9a-f]{64}$') { return $Value }; return $null }
function Read-SafeStage($Value) {
    $status = Read-Field $Value 'status'; $id = Read-Field $Value 'diagnostic_id'
    if ($status -isnot [string] -or @('passed','failed','unavailable') -cnotcontains $status) { $status = 'unavailable' }
    if ($id -isnot [string] -or $SafeDiagnosticIds -cnotcontains $id) { $id = $null }
    return [ordered]@{ status = $status; diagnostic_id = $id }
}
function Read-NumericObject($Value, [string[]]$Names, [long]$Maximum) {
    if ($null -eq $Value) { return $null }
    $r = [ordered]@{}
    foreach ($name in $Names) { $r[$name] = Read-SafeInteger (Read-Field $Value $name) $Maximum }
    return $r
}
function ConvertTo-SafeDiagnostic([string]$Text) {
    try { $obj = ConvertFrom-Json -InputObject $Text -ErrorAction Stop } catch { return [ordered]@{ status = 'invalid_json'; report = $null } }
    if ((Read-Field $obj 'format') -cne 'benilla-terrain-diagnostic' -or (Read-SafeInteger (Read-Field $obj 'version') 1) -ne 1) {
        return [ordered]@{ status = 'unsupported_schema'; report = $null }
    }
    $lease = Read-Field $obj 'live_install_lease'; $admitted = Read-Field $obj 'gameplay_admitted'
    if ($lease -isnot [bool] -or $lease -ne $false -or $admitted -isnot [bool] -or $admitted -ne $false) {
        return [ordered]@{ status = 'invalid_safety_fields'; report = $null }
    }
    $r = [ordered]@{ format = 'benilla-terrain-diagnostic'; version = 1;
        map = (Read-SafeInteger (Read-Field $obj 'map') 999);
        map_table = (Read-NumericObject (Read-Field $obj 'map_table') @('selected_row_ordinal','total_rows') 4294967295);
        source_epoch = (Read-SafeHash (Read-Field $obj 'source_epoch'));
        live_install_lease = $false; gameplay_admitted = $false; stages = [ordered]@{}; tiles = @() }
    foreach ($stage in @('selection','initial_capture','map_table','selection_capture','full_capture','area_table','liquid_table','source_recheck')) {
        $r.stages[$stage] = Read-SafeStage (Read-Field (Read-Field $obj 'stages') $stage)
    }
    $tiles = @(Read-Field $obj 'tiles')
    if ($tiles.Count -gt 9) { return [ordered]@{ status = 'tile_count_limit'; report = $null } }
    foreach ($t in $tiles) {
        if ($null -eq $t) { continue }
        $raw = Read-Field $t 'raw_adt'
        $safeRaw = $null
        if ($null -ne $raw) { $safeRaw = [ordered]@{ bytes = (Read-SafeInteger (Read-Field $raw 'bytes') 1073741824); sha256 = (Read-SafeHash (Read-Field $raw 'sha256')) } }
        $s = [ordered]@{ x = (Read-SafeInteger (Read-Field $t 'x') 63); y = (Read-SafeInteger (Read-Field $t 'y') 63);
            raw_adt = $safeRaw; active_mclq_cells = (Read-SafeInteger (Read-Field $t 'active_mclq_cells') 256);
            liquid_rectangle = (Read-NumericObject (Read-Field $t 'liquid_rectangle') @('x','y','width','height') 129);
            missing_liquid_heights = (Read-NumericObject (Read-Field $t 'missing_liquid_heights') @('count','first_x','first_y') 16641);
            history = (Read-NumericObject (Read-Field $t 'history') @('consulted_adts','consulted_wdts','absent_members','initial_zero_samples') 4294967295);
            stages = [ordered]@{} }
        foreach ($stage in @('wdt_selection','adt_structure','liquid','history_resolution','production')) { $s.stages[$stage] = Read-SafeStage (Read-Field (Read-Field $t 'stages') $stage) }
        $r.tiles += $s
    }
    return [ordered]@{ status = 'recognized_schema_filtered'; report = $r }
}
function ConvertTo-RunMetadata($Run) {
    $r = [ordered]@{}
    foreach ($key in @('status','exit_code','elapsed_ms','timed_out','termination_attempted','termination_confirmed','stdout_characters_seen','stderr_characters_seen','stdout_truncated','stderr_truncated')) { $r[$key] = $Run[$key] }
    return $r
}

$reportStream = $null
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'platform.windows_required' }
    $xy = $Tile.Split(','); $tileX = [int]$xy[0]; $tileY = [int]$xy[1]
    if ($tileX -gt 63 -or $tileY -gt 63) { throw 'selection.tile_bounds' }
    $Tile = [string]$tileX + ',' + [string]$tileY
    $dataPath = Get-LocalFullPath $Data; $helperPath = Get-LocalFullPath $Helper; $outputPath = Get-LocalFullPath $Output
    if ([IO.Path]::GetExtension($outputPath) -ine '.json') { throw 'output.json_extension_required' }
    if (Test-InDirectory $outputPath $dataPath) { throw 'output.inside_data_refused' }
    if (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($outputPath))) { throw 'output.parent_missing' }
    if (Test-ReparseAncestor ([IO.Path]::GetDirectoryName($outputPath))) { throw 'output.reparse_path_refused' }
    if ([IO.File]::Exists($outputPath) -or [IO.Directory]::Exists($outputPath)) { throw 'output.already_exists' }
    # CreateNew is authoritative against races; never overwrite or delete an output.
    $reportStream = [IO.File]::Open($outputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $dataStatus = 'available'
    if (-not [IO.Directory]::Exists($dataPath)) { $dataStatus = 'missing' }
    elseif (Test-ReparseAncestor $dataPath) { $dataStatus = 'reparse_path_refused' }
    $dataAvailable = $dataStatus -eq 'available'
    $edition = 'Desktop'; if ($PSVersionTable.ContainsKey('PSEdition')) { $edition = $PSVersionTable.PSEdition }
    $identity = Get-HelperIdentity $helperPath
    $report = [ordered]@{
        format = 'benilla-terrain-windows-diagnostics'; version = 1; collected_utc = [DateTime]::UtcNow.ToString('o');
        scope = [ordered]@{ data = '[DATA]'; helper = '[HELPER]'; output = '[REPORT]'; map = $Map; tile_x = $tileX; tile_y = $tileY;
            data_status = $dataStatus; writes = 'one_new_json_report_only'; archive_content_extraction = $false;
            network_requests = $false; full_archive_hashes = 'skipped'; full_world_scan = $false };
        environment = [ordered]@{ powershell_version = $PSVersionTable.PSVersion.ToString(); powershell_edition = $edition;
            os_version = [Environment]::OSVersion.Version.ToString(); os_64_bit = [Environment]::Is64BitOperatingSystem;
            process_64_bit = [Environment]::Is64BitProcess };
        helper = $identity; archive_layout = (Get-ArchiveLayout $dataPath $dataAvailable); archive_envelopes = @();
        helper_capability_probe = $null; helper_run = $null;
        detailed_diagnostics = [ordered]@{ status = 'unavailable'; report = $null };
        limitations = @('Only the selected map and tile are requested.', 'Archive envelopes do not prove member readability or content correctness.',
            'Older inspect helpers stop at the first refusal; deeper dependent stages remain unknown.',
            'The helper may inspect a bounded predecessor dependency closure; this collector does not perform a separate world scan.',
            'A completed diagnostic process does not establish gameplay admission.',
            'Only allowlisted structured fields and sanitized text are retained; unknown helper output is omitted.')
    }
    foreach ($name in @('model.MPQ','dbc.MPQ','terrain.MPQ','patch.MPQ','patch-2.MPQ')) { $report.archive_envelopes += Get-ArchiveEnvelope $dataPath $name $dataAvailable }
    if ($SkipHelper) { $report.helper_run = [ordered]@{ status = 'skipped_by_request' } }
    elseif (-not $dataAvailable) { $report.helper_run = [ordered]@{ status = 'skipped_data_unavailable' } }
    elseif ($identity.status -ne 'available') { $report.helper_run = [ordered]@{ status = 'skipped_helper_unavailable' } }
    else {
        $help = Invoke-BoundedHelper $helperPath @('--help') 10
        $probe = ConvertTo-RunMetadata $help
        $probe['diagnose_advertised'] = $help.status -eq 'completed' -and $help.exit_code -eq 0 -and $help.stdout -match '(?m)^\s*--diagnose(?:\s|$)'
        $report.helper_capability_probe = $probe
        if ($help.status -ne 'completed' -or $help.exit_code -ne 0) { $report.helper_run = [ordered]@{ status = 'skipped_capability_probe_failed' } }
        else {
            $mode = $(if ($probe.diagnose_advertised) { '--diagnose' } else { '--inspect' })
            $run = Invoke-BoundedHelper $helperPath @('--data', $dataPath, '--map', [string]$Map, '--tile', $Tile, $mode) $TimeoutSeconds
            $runReport = ConvertTo-RunMetadata $run; $runReport['mode'] = $mode
            $runReport['stderr'] = ConvertTo-SafeText $run.stderr
            if ($mode -eq '--diagnose') {
                $runReport['stdout'] = [ordered]@{ text = '[structured output filtered into detailed_diagnostics]'; unrecognized_lines_omitted = $null }
                if (-not $run.stdout_truncated -and $run.status -eq 'completed') { $report.detailed_diagnostics = ConvertTo-SafeDiagnostic $run.stdout }
                else { $report.detailed_diagnostics = [ordered]@{ status = 'incomplete_helper_output'; report = $null } }
            } else {
                $runReport['stdout'] = ConvertTo-SafeText $run.stdout
                $report.detailed_diagnostics = [ordered]@{ status = 'unavailable_in_older_helper'; report = $null }
            }
            $report.helper_run = $runReport
        }
    }
    if ($identity.status -eq 'available') {
        try {
            $identity.sha256_after_collection = Get-Sha256 $helperPath
            $identity.unchanged_after_collection = $identity.sha256 -ceq $identity.sha256_after_collection
        } catch { $identity.unchanged_after_collection = $false }
    }
    $json = ConvertTo-Json -InputObject $report -Depth 16
    $encoding = New-Object Text.UTF8Encoding($false)
    $bytes = $encoding.GetBytes($json + "`r`n")
    $reportStream.Write($bytes, 0, $bytes.Length); $reportStream.Flush()
    if ($report.helper_run.Contains('termination_confirmed') -and $report.helper_run.termination_confirmed -eq $false) {
        Write-Warning 'The helper could not be confirmed stopped. Check for a running benilla-native-terrain process.'
    }
    Write-Output 'Diagnostic report created. Review the JSON, then share it; the selected Data and helper paths are not included.'
} catch {
    # Never echo arbitrary exceptions, paths, environment strings, or helper output.
    $known = @('input.local_drive_required','input.network_drive_refused','input.alternate_stream_refused','platform.windows_required','selection.tile_bounds',
        'output.json_extension_required','output.inside_data_refused','output.parent_missing','output.reparse_path_refused','output.already_exists')
    $id = 'collector.failed'; if ($known -contains $_.Exception.Message) { $id = $_.Exception.Message }
    throw ('Benilla diagnostic collector stopped [' + $id + ']. No existing file was overwritten. If a new report was created, it may be incomplete.')
} finally { if ($null -ne $reportStream) { $reportStream.Dispose() } }
