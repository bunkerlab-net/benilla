use benilla_extractor_observer::{observe, Plan};

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("usage: benilla-extractor-observer PLAN.json NEW_RECEIPT_DIRECTORY");
        return std::process::ExitCode::from(2);
    }
    let result = std::fs::read(&args[1])
        .map_err(|error| error.to_string())
        .and_then(|data| serde_json::from_slice::<Plan>(&data).map_err(|error| error.to_string()))
        .and_then(|plan| {
            observe(&plan, std::path::Path::new(&args[2])).map_err(|error| error.to_string())
        });
    match result {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(1)
        }
    }
}
