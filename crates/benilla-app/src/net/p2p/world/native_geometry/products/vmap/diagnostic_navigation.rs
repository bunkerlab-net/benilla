// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded read-only navigation observations; no cache publication or gameplay admission.
use super::super::super::nav_generation::{GenerationError, PreparedNavigationInput};
use super::*;
use benilla_native_nav_generator::GenerationStatus;

pub(super) fn run<W: Write>(
    report: &mut Report<'_, W>,
    initial: Option<&terrain_source::PreparedTerrain>,
    bundle: Option<&source::AssembledSourceMap<'_>>,
    points: &[Option<[u32; 3]>; 9],
    deadline: Instant,
    helper: Option<PathBuf>,
) -> io::Result<usize> {
    let selected = report.selection.clone();
    let cancelled = || Instant::now() >= deadline;
    let mut tiles = vec![terrain_source::Tile {
        x: selected.x,
        y: selected.y,
    }];
    let mut usable = initial.is_some() && bundle.is_some();
    for (index, (dx, dy)) in [(0i16, 1i16), (0, -1), (1, 0), (-1, 0)]
        .into_iter()
        .enumerate()
    {
        let stage = format!("navigation_neighbor_{index}");
        let x = i16::from(selected.x) + dx;
        let y = i16::from(selected.y) + dy;
        if initial.is_none() || bundle.is_none() {
            report.simple(&stage, "unavailable", Some("collision.prerequisite"))?;
        } else if cancelled() {
            usable = false;
            report.simple(&stage, "failed", Some("navigation.deadline"))?;
        } else if !(0..64).contains(&x) || !(0..64).contains(&y) {
            report.record(
                &stage,
                "passed",
                None,
                json!({"neighbor_index":index,"neighbor_present":0}),
                json!({}),
                Value::Null,
                Value::Null,
            )?;
        } else {
            let tile = terrain_source::Tile {
                x: x as u8,
                y: y as u8,
            };
            // Source extraction is synchronous and bounded; the collector supplies
            // the enclosing hard deadline. Missing WDT tiles are explicit absence.
            match terrain_source::prepare(&selected.data, selected.map, &[tile]) {
                Ok(capture) => {
                    if let Err(error) = capture.verify_unchanged() {
                        usable = false;
                        report.simple(
                            &stage,
                            "failed",
                            Some(terrain_source::diagnostic_id(&error)),
                        )?;
                        continue;
                    }
                    tiles.push(tile);
                    report.record(
                        &stage,
                        "passed",
                        None,
                        json!({"neighbor_index":index,"neighbor_present":1}),
                        json!({}),
                        Value::Null,
                        Value::Null,
                    )?;
                }
                Err(error) => {
                    let code = terrain_source::diagnostic_id(&error);
                    if code == "terrain.wdt_tile_absent" {
                        report.record(
                            &stage,
                            "passed",
                            None,
                            json!({"neighbor_index":index,"neighbor_present":0}),
                            json!({}),
                            Value::Null,
                            Value::Null,
                        )?;
                    } else {
                        usable = false;
                        report.simple(&stage, "failed", Some(code))?;
                    }
                }
            }
        }
    }
    let terrain = if usable && !cancelled() {
        match terrain_source::prepare(&selected.data, selected.map, &tiles) {
            Ok(capture) => {
                report.record(
                    "navigation_terrain",
                    "passed",
                    None,
                    json!({"navigation_tiles":capture.tiles().count()}),
                    json!({"terrain_source_epoch":capture.source_epoch()}),
                    Value::Null,
                    Value::Null,
                )?;
                Some(capture)
            }
            Err(error) => {
                report.simple(
                    "navigation_terrain",
                    "failed",
                    Some(terrain_source::diagnostic_id(&error)),
                )?;
                None
            }
        }
    } else {
        report.simple(
            "navigation_terrain",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
        None
    };
    let input = if let (Some(terrain), Some(bundle)) = (&terrain, bundle) {
        match PreparedNavigationInput::from_source(
            terrain,
            bundle,
            TileId {
                x: selected.x,
                y: selected.y,
            },
            cancelled,
        ) {
            Ok(input) => match input.summary() {
                Ok(s) => {
                    report.record("navigation_input","passed",None,json!({"navigation_tiles":s.tiles,"solid_vertices":s.solid_vertices,"solid_triangles":s.solid_triangles,"liquid_vertices":s.liquid_vertices,"liquid_triangles":s.liquid_triangles,"request_bytes":s.request_bytes}),json!({"navigation_input":hex(&s.revision)}),Value::Null,Value::Null)?;
                    Some(input)
                }
                Err(error) => {
                    report.simple("navigation_input", "failed", Some(code(&error)))?;
                    None
                }
            },
            Err(error) => {
                report.simple("navigation_input", "failed", Some(code(&error)))?;
                None
            }
        }
    } else {
        report.simple(
            "navigation_input",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
        None
    };
    let output = if let Some(input) = &input {
        let mut observed = None;
        let helper_valid = helper
            .as_ref()
            .is_some_and(|path| trusted_helper_path(path));
        let result = if !benilla_native_nav::SUPPORTED_ABI {
            Err(GenerationError::Native(
                benilla_native_nav::Error::Unsupported,
            ))
        } else if helper_valid {
            input.generate_observed(helper.as_ref().unwrap(), deadline, cancelled, |summary| {
                observed = Some(summary)
            })
        } else {
            Err(GenerationError::Io)
        };
        if let Some(s) = observed {
            let status = match s.status {
                GenerationStatus::Generated => 0,
                GenerationStatus::Empty => 1,
                GenerationStatus::Exact625 => 2,
            };
            report.record("navigation_generation","passed",None,json!({"native_peak_bytes":s.native_peak_bytes,"navigation_output_bytes":s.output_bytes,"navigation_polygons":s.polygon_count,"navigation_status":status}),json!({}),Value::Null,Value::Null)?;
        } else {
            let reason = if !benilla_native_nav::SUPPORTED_ABI {
                "navigation.unsupported"
            } else if !helper_valid {
                "navigation.helper_missing"
            } else {
                result
                    .as_ref()
                    .err()
                    .map(code)
                    .unwrap_or("navigation.protocol")
            };
            report.simple("navigation_generation", "failed", Some(reason))?;
        }
        match result {
            Ok(output) if output.summary().status == GenerationStatus::Generated => {
                report.record(
                    "navigation_validation",
                    "passed",
                    None,
                    json!({}),
                    json!({"navigation_output":hex(&output.revision())}),
                    Value::Null,
                    Value::Null,
                )?;
                Some(output)
            }
            Ok(output) => {
                report.simple(
                    "navigation_validation",
                    "unavailable",
                    Some(if output.summary().status == GenerationStatus::Exact625 {
                        "navigation.exact625"
                    } else {
                        "navigation.empty"
                    }),
                )?;
                None
            }
            Err(error) => {
                report.simple(
                    "navigation_validation",
                    if observed.is_some() {
                        "failed"
                    } else {
                        "unavailable"
                    },
                    Some(if observed.is_some() {
                        code(&error)
                    } else {
                        "collision.prerequisite"
                    }),
                )?;
                None
            }
        }
    } else {
        report.simple(
            "navigation_generation",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
        report.simple(
            "navigation_validation",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
        None
    };
    let mut completed = 0;
    if let Some(output) = output {
        match output.load() {
            Ok(mut mesh) => {
                for (index, point) in points
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.map(|p| (i, p)))
                {
                    let reply = mesh.nearest(point, 5.0, deadline);
                    let (status, reason, target, result, metrics) = match reply {
                        Ok(reply) => {
                            completed += 1;
                            (
                                "passed",
                                None,
                                Some(reply.point),
                                "nearest",
                                json!({"polygon_ref_low":reply.polygon as u32,"polygon_ref_high":(reply.polygon>>32) as u32}),
                            )
                        }
                        Err(benilla_native_nav::Error::Unavailable) => {
                            completed += 1;
                            ("passed", None, None, "miss", json!({}))
                        }
                        Err(error) => {
                            ("failed", Some(native_code(error)), None, "error", json!({}))
                        }
                    };
                    report.record("navigation_probe",status,reason,metrics,json!({}),json!({"index":index,"kind":"nearest","from_bits":point,"to_bits":target,"result":result,"value_bits":null}),Value::Null)?;
                    if cancelled() {
                        break;
                    }
                }
                if points.iter().all(Option::is_none) {
                    report.simple(
                        "navigation_probe",
                        "unavailable",
                        Some("collision.prerequisite"),
                    )?;
                }
            }
            Err(error) => report.simple("navigation_probe", "failed", Some(code(&error)))?,
        }
    } else {
        report.simple(
            "navigation_probe",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
    }
    if let Some(input) = &input {
        match input.verify_unchanged(cancelled) {
            Ok(()) => report.simple("navigation_freshness", "passed", None)?,
            Err(error) => report.simple("navigation_freshness", "failed", Some(code(&error)))?,
        }
    } else {
        report.simple(
            "navigation_freshness",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
    }
    Ok(completed)
}
fn code(error: &GenerationError) -> &'static str {
    match error {
        GenerationError::Cancelled => "navigation.cancelled",
        GenerationError::Deadline => "navigation.deadline",
        GenerationError::Io => "navigation.io",
        GenerationError::Protocol => "navigation.protocol",
        GenerationError::Helper => "navigation.helper",
        GenerationError::Validation(super::super::super::Error::Budget) => "navigation.budget",
        GenerationError::Validation(super::super::super::Error::Missing) => "navigation.missing",
        GenerationError::Validation(super::super::super::Error::Unsupported) => {
            "navigation.unsupported"
        }
        GenerationError::Validation(_) => "navigation.input",
        GenerationError::Composition(_) => "navigation.source_compatibility",
        GenerationError::Collision(_) => "navigation.collision",
        GenerationError::Native(error) => native_code(*error),
    }
}
fn native_code(error: benilla_native_nav::Error) -> &'static str {
    use benilla_native_nav::Error;
    match error {
        Error::Format => "navigation.native_format",
        Error::Framing => "navigation.native_framing",
        Error::Unsupported => "navigation.unsupported",
        Error::Identity => "navigation.native_identity",
        Error::Budget => "navigation.budget",
        Error::Deadline => "navigation.deadline",
        Error::Unavailable => "navigation.missing",
        Error::Native(_) => "navigation.native_query",
    }
}

// Observational path check, not a sandbox or race-free executable attestation.
fn trusted_helper_path(path: &std::path::Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let drive = match path.components().next() {
            Some(Component::Prefix(p)) => match p.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
                _ => return false,
            },
            _ => return false,
        };
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
        // SAFETY: root is a live, NUL-terminated UTF-16 drive-root string.
        // Reject remote, missing and unknown drives before touching their paths.
        if !matches!(unsafe { GetDriveTypeW(root.as_ptr()) }, 2 | 3 | 5 | 6) {
            return false;
        }
    }
    // Check parents first so a reparse ancestor is refused before following it.
    let ancestors: Vec<_> = path.ancestors().collect();
    for component in ancestors.into_iter().rev() {
        let Ok(metadata) = std::fs::symlink_metadata(component) else {
            return false;
        };
        if metadata.file_type().is_symlink() || (component == path && !metadata.is_file()) {
            return false;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod helper_path_tests {
    use super::*;
    #[test]
    fn current_executable_is_regular_but_missing_and_relative_paths_are_refused() {
        assert!(trusted_helper_path(&std::env::current_exe().unwrap()));
        assert!(!trusted_helper_path(std::path::Path::new(
            "benilla-native-navigation"
        )));
        assert!(!trusted_helper_path(
            &std::env::current_exe().unwrap().join("missing")
        ));
    }
    #[cfg(unix)]
    #[test]
    fn helper_symlink_is_refused() {
        let root = std::env::temp_dir().join(format!("benilla-helper-link-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let link = root.join("navigation");
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), &link).unwrap();
        assert!(!trusted_helper_path(&link));
        let parent_link = root.join("parent");
        std::os::unix::fs::symlink(
            std::env::current_exe().unwrap().parent().unwrap(),
            &parent_link,
        )
        .unwrap();
        assert!(!trusted_helper_path(
            &parent_link.join(std::env::current_exe().unwrap().file_name().unwrap())
        ));
        std::fs::remove_file(parent_link).unwrap();
        std::fs::remove_file(link).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

#[cfg(all(test, windows))]
mod windows_helper_path_tests {
    use super::*;
    #[test]
    fn unc_and_device_paths_are_refused_without_access() {
        assert!(!trusted_helper_path(std::path::Path::new(
            r"\\invalid.example\share\navigation.exe"
        )));
        assert!(!trusted_helper_path(std::path::Path::new(
            r"\\?\UNC\invalid.example\share\navigation.exe"
        )));
        assert!(!trusted_helper_path(std::path::Path::new(
            r"\\.\navigation"
        )));
    }
    #[test]
    fn directory_junction_ancestor_is_refused() {
        let root =
            std::env::temp_dir().join(format!("benilla-helper-junction-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("navigation.exe"), []).unwrap();
        let junction = root.join("junction");
        let status = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!trusted_helper_path(&junction.join("navigation.exe")));
        std::fs::remove_dir(&junction).unwrap();
        std::fs::remove_file(target.join("navigation.exe")).unwrap();
        std::fs::remove_dir(target).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
