// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use benilla_native_terrain::{prepare, Tile};
use clap::{error::ErrorKind, ArgGroup, CommandFactory, Parser};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, io::Write as _, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "benilla-native-terrain",
    bin_name = "benilla-native-terrain",
    about = "Inspect or privately export selected terrain from your own Classic Data; no gameplay activation",
    group(ArgGroup::new("mode").required(true).args(["inspect", "output"]))
)]
struct Args {
    #[arg(long)]
    data: PathBuf,
    /// Validate and generate in memory, without publishing or writing artifacts.
    #[arg(long)]
    inspect: bool,
    /// Publish to a fresh private directory outside the client installation.
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    map: u32,
    /// Terrain grid pair X,Y; repeat at most nine times. ADT filename order is reversed.
    #[arg(long = "tile", required = true, value_parser = parse_tile)]
    tiles: Vec<Tile>,
}

fn parse_tile(s: &str) -> Result<Tile, String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok(Tile {
        x: x.parse().map_err(|_| "invalid X")?,
        y: y.parse().map_err(|_| "invalid Y")?,
    })
}

// Only these literal, reviewed contexts can become public diagnostic IDs. Never
// echo paths, source-member suffixes, arbitrary dotted words, or anyhow chains.
const SAFE_DIAGNOSTIC_IDS: &[&str] = &[
    "output.already_exists",
    "output.changed_product",
    "output.create_new_directory",
    "output.directory_name",
    "output.inside_source_installation",
    "output.parent_must_exist",
    "output.published_but_durability_unconfirmed",
    "selection.bounds",
    "selection.duplicate_tile",
    "selection.tile_bounds",
    "source.ad_archive_case_ambiguity",
    "source.ad_archive_metadata",
    "source.ad_archive_size",
    "source.ad_bytes_changed",
    "source.ad_deleted",
    "source.ad_directory_entry_limit",
    "source.ad_dummy_member",
    "source.ad_hash_count",
    "source.ad_header_extension",
    "source.ad_header_open",
    "source.ad_header_read",
    "source.ad_header_scan_limit",
    "source.ad_header_selection",
    "source.ad_header_signature",
    "source.ad_header_truncated",
    "source.ad_header_version",
    "source.ad_index_budget",
    "source.ad_index_spans",
    "source.ad_member_profile",
    "source.ad_missing_member",
    "source.ad_missing_size",
    "source.ad_open_archive",
    "source.ad_read",
    "source.ad_regular_archive_required",
    "source.ad_sector_size",
    "source.ad_winner_mismatch",
    "source.adt_count_limit",
    "source.aggregate_byte_limit",
    "source.archive_basename",
    "source.bytes_changed",
    "source.data_directory",
    "source.data_directory_required",
    "source.discovery_changed",
    "source.discovery_directory_changed",
    "source.discovery_missing",
    "source.duplicate_member",
    "source.installation_parent",
    "source.map_directory",
    "source.member_byte_limit",
    "source.member_count_limit",
    "source.member_length_changed",
    "source.member_name_limit",
    "source.missing_or_deleted",
    "source.missing_size",
    "source.multiple_maps",
    "source.not_captured",
    "source.open_patch_chain",
    "source.read",
    "source.recheck_ad_profile",
    "source.recheck_members",
    "source.recheck_open",
    "source.recheck_read",
    "source.tile_coordinate",
    "source.unselected_member_kind",
    "source.wdt_count_limit",
    "source.winner_changed",
    "terrain.AreaTable",
    "terrain.LiquidType",
    "terrain.Map",
    "terrain.adt_mhdr",
    "terrain.adt_required_records",
    "terrain.adt_unsupported_or_liquid",
    "terrain.adt_version",
    "terrain.canonical_mcin_required",
    "terrain.cell_count",
    "terrain.chunk_count",
    "terrain.chunk_framing",
    "terrain.chunk_header",
    "terrain.chunk_overflow",
    "terrain.chunk_payload",
    "terrain.dangling_root_chunk",
    "terrain.dangling_subchunk",
    "terrain.dbc_framing",
    "terrain.dbc_id_budget",
    "terrain.dbc_layout",
    "terrain.dbc_magic",
    "terrain.dbc_nul",
    "terrain.dbc_rows",
    "terrain.dbc_size",
    "terrain.dbc_string_nul",
    "terrain.dbc_string_offset",
    "terrain.dbc_string_utf8",
    "terrain.dbc_strings",
    "terrain.duplicate_chunk",
    "terrain.duplicate_map_id",
    "terrain.duplicate_mcin_reference",
    "terrain.duplicate_subchunk",
    "terrain.height_overflow",
    "terrain.liquid_empty_bounds",
    "terrain.liquid_excluded",
    "terrain.liquid_history",
    "terrain.liquid_nonfinite",
    "terrain.map_directory",
    "terrain.map_missing",
    "terrain.mccv_excluded",
    "terrain.mcin_not_mcnk",
    "terrain.mcin_size",
    "terrain.mclq_length",
    "terrain.mclq_size",
    "terrain.mcnk_header",
    "terrain.mcnk_unsupported_or_liquid",
    "terrain.mcvt_length",
    "terrain.member_budget",
    "terrain.mfbo_excluded",
    "terrain.mh2o_excluded",
    "terrain.mhdr_reference",
    "terrain.nonfinite_height",
    "terrain.normal_padding",
    "terrain.offset_overflow",
    "terrain.subchunk_reference",
    "terrain.tile_coordinate",
    "terrain.truncated_half",
    "terrain.truncated_word",
    "terrain.wdt_global_or_header",
    "terrain.wdt_global_wmo",
    "terrain.wdt_main",
    "terrain.wdt_required_records",
    "terrain.wdt_tile_absent",
    "terrain.wdt_unsupported_record",
    "terrain.wdt_version",
    "terrain.writer_framing",
];

fn diagnostic_id(error: &anyhow::Error) -> &'static str {
    for cause in error.chain().take(16) {
        let message = cause.to_string();
        let candidate = message
            .split_once(':')
            .map_or(message.as_str(), |(id, _)| id);
        if let Some(id) = SAFE_DIAGNOSTIC_IDS.iter().find(|id| **id == candidate) {
            return id;
        }
    }
    "terrain.operation_failed"
}

fn operation_diagnostic(error: &anyhow::Error) -> String {
    let id = diagnostic_id(error);
    let explanation = match id {
        "source.data_directory_required" | "source.data_directory" => {
            "The Data directory is missing or unavailable"
        }
        "source.ad_header_signature" => "No MPQ header was found on a 512-byte boundary",
        "source.ad_header_truncated" => "The selected MPQ header is incomplete for its version",
        "source.ad_header_version" => "The first MPQ header has an unsupported raw version",
        "source.ad_header_extension" => "The MPQ version-one extended offsets must all be zero",
        "source.ad_header_scan_limit" => "MPQ header discovery exceeds the one MiB scan budget",
        "source.ad_header_selection" => {
            "The reader and pinned extractor select different MPQ headers"
        }
        "source.ad_archive_size" => {
            "The declared MPQ size does not match the bytes after its selected header"
        }
        "source.ad_sector_size" => "The MPQ sector size exceeds the supported 64 KiB limit",
        "source.ad_hash_count" => "The MPQ hash count must be a nonzero power of two",
        "source.ad_index_budget" => "The MPQ index exceeds the 64 MiB budget",
        "source.ad_index_spans" => "The MPQ index spans overlap or fall outside the archive",
        "output.published_but_durability_unconfirmed" => {
            "The complete export marker may already be visible; durability is unconfirmed"
        }
        "output.already_exists" => "The output directory already exists; choose a fresh directory",
        "output.inside_source_installation" => {
            "The output must be outside the entire client installation"
        }
        "output.parent_must_exist" => "The output parent directory must exist and be accessible",
        "selection.bounds" => "Select map 0 through 999 and one through nine tiles",
        "selection.tile_bounds" => "Tile coordinates must each be 0 through 63",
        "selection.duplicate_tile" => "Select each tile at most once",
        _ => "Terrain validation or export failed",
    };
    format!("error[{id}]: {explanation}; no gameplay admission established\n")
}

fn argument_diagnostic(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ArgumentConflict => {
            "error[cli.mode_conflict]: Choose exactly one of --inspect or --output\n"
        }
        ErrorKind::MissingRequiredArgument => {
            "error[cli.missing_argument]: Supply --data, --map, at least one --tile, and exactly one of --inspect or --output\n"
        }
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::InvalidUtf8 => {
            "error[cli.invalid_value]: Check option values; --map is an integer and --tile is X,Y\n"
        }
        ErrorKind::UnknownArgument => {
            "error[cli.unknown_argument]: Unrecognized option; use --help for supported options\n"
        }
        _ => "error[cli.invalid_arguments]: Invalid arguments; use --help for supported options\n",
    }
}

fn run(args: Args) -> Result<String> {
    // Both modes consume the same bounded capture, validation, and pinned writer.
    // Inspection never calls publication or creates an output directory.
    let prepared = prepare(&args.data, args.map, &args.tiles)?;
    let mut report = String::new();
    let count = prepared.tiles().count();
    if args.inspect {
        writeln!(
            report,
            "Inspected {count} terrain tiles in memory; no artifacts published"
        )?;
    } else {
        writeln!(
            report,
            "Produced {count} terrain tiles in a fresh private export"
        )?;
    }
    writeln!(
        report,
        "map={} source_epoch={}",
        prepared.map_id(),
        prepared.source_epoch()
    )?;
    for (tile, bytes) in prepared.tiles() {
        writeln!(
            report,
            "tile={},{} bytes={} sha256={:x}",
            tile.x,
            tile.y,
            bytes.len(),
            Sha256::digest(bytes)
        )?;
    }
    report.push_str("live_install_lease=false gameplay_admitted=false\nNo live installation lease or gameplay admission established\n");
    if let Some(output) = args.output {
        prepared.publish(&output)?;
    }
    Ok(report)
}

fn main() -> ExitCode {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            // Regenerate help from the fixed command, never clap's user-input error.
            let help = Args::command().render_help().to_string();
            return write_stdout(&help);
        }
        Err(error) => {
            let _ = std::io::stderr()
                .lock()
                .write_all(argument_diagnostic(error.kind()).as_bytes());
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(report) => write_stdout(&report),
        Err(error) => {
            let _ = std::io::stderr()
                .lock()
                .write_all(operation_diagnostic(&error).as_bytes());
            ExitCode::FAILURE
        }
    }
}

fn write_stdout(text: &str) -> ExitCode {
    if std::io::stdout().lock().write_all(text.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        let _ = std::io::stderr().lock().write_all(
            b"error[cli.report_failed]: Could not write the report; an export may already exist\n",
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_mode_is_required() {
        let base = [
            "benilla-native-terrain",
            "--data",
            "/private/Data",
            "--map",
            "1",
            "--tile",
            "31,32",
        ];
        assert_eq!(
            Args::try_parse_from(base).err().unwrap().kind(),
            ErrorKind::MissingRequiredArgument
        );
        let inspect = Args::try_parse_from(base.into_iter().chain(["--inspect"])).unwrap();
        assert!(inspect.inspect);
        assert!(inspect.output.is_none());
        let export =
            Args::try_parse_from(base.into_iter().chain(["--output", "/private/output"])).unwrap();
        assert!(!export.inspect);
        assert_eq!(export.output, Some(PathBuf::from("/private/output")));
        assert_eq!(
            Args::try_parse_from(base.into_iter().chain([
                "--inspect",
                "--output",
                "/private/output"
            ]))
            .err()
            .unwrap()
            .kind(),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn safe_context_keeps_only_the_allowlisted_identifier() {
        let error = anyhow::anyhow!("failed to read /private/player/account/Data/terrain.MPQ")
            .context("source.read:World\\Maps\\PrivatePlayerName\\secret.adt");
        assert_eq!(diagnostic_id(&error), "source.read");
        assert_eq!(
            operation_diagnostic(&error),
            "error[source.read]: Terrain validation or export failed; no gameplay admission established\n"
        );
    }

    #[test]
    fn liquid_diagnostics_retain_only_reviewed_ids() {
        for id in [
            "terrain.mclq_length",
            "terrain.mclq_size",
            "terrain.liquid_history",
            "terrain.liquid_empty_bounds",
            "terrain.liquid_nonfinite",
        ] {
            let error = anyhow::anyhow!(format!("{id}:/private/account/World/secret.adt"));
            let diagnostic = operation_diagnostic(&error);
            assert!(diagnostic.starts_with(&format!("error[{id}]:")));
            assert!(diagnostic.contains("no gameplay admission established"));
            assert!(!diagnostic.contains("private"));
            assert!(!diagnostic.contains("secret"));
            assert!(diagnostic.len() < 256);
        }
    }

    #[test]
    fn arbitrary_dotted_messages_and_known_prefix_collisions_are_not_ids() {
        for message in [
            "source.private_player_name:/private/account",
            "source.read_extra:/private/account",
            "source.read/private/account",
            "source.read\n/private/account",
            "/private/account:source.read",
            "terrain.json",
        ] {
            let error = anyhow::anyhow!(message.to_owned());
            assert_eq!(diagnostic_id(&error), "terrain.operation_failed");
        }
        let error = anyhow::anyhow!(format!("source.{}", "private".repeat(100_000)));
        assert!(operation_diagnostic(&error).len() < 256);
    }

    #[test]
    fn publication_warning_is_not_hidden_by_a_lower_level_context() {
        let error = anyhow::anyhow!("source.read:/private/account")
            .context("output.published_but_durability_unconfirmed");
        let diagnostic = operation_diagnostic(&error);
        assert!(diagnostic.contains("error[output.published_but_durability_unconfirmed]"));
        assert!(diagnostic.contains("may already be visible"));
        assert!(!diagnostic.contains("private"));
        assert!(diagnostic.len() < 256);
    }

    #[test]
    fn every_argument_error_has_a_bounded_static_message() {
        for kind in [
            ErrorKind::ArgumentConflict,
            ErrorKind::MissingRequiredArgument,
            ErrorKind::InvalidValue,
            ErrorKind::ValueValidation,
            ErrorKind::InvalidUtf8,
            ErrorKind::UnknownArgument,
            ErrorKind::TooManyValues,
        ] {
            let message = argument_diagnostic(kind);
            assert!(message.len() < 256);
            assert!(message.starts_with("error[cli."));
            assert!(!message.contains('/'));
        }
    }
}
