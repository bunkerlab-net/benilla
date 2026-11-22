//! Player-accessible recovery before Bevy, config loading, asset discovery, or networking.
use crate::{AppExit, BuildId};

pub(crate) fn run(build: BuildId) -> Option<AppExit> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("characters")) {
        return None;
    }
    benilla_formats::set_project_folder(build.project_dir);
    let directory = crate::local_state::home().map(|home| home.join("p2p-profile"));
    let result = benilla_profile::recovery::run(
        &args.collect::<Vec<_>>(),
        directory.as_deref(),
        &mut std::io::stdout().lock(),
    );
    Some(match result {
        Ok(()) => AppExit::Success,
        Err(error) => {
            eprintln!("benilla: {error}");
            AppExit::error()
        }
    })
}
