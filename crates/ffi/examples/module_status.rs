//! `module_status <pid> [wait] [seconds]`: injects `bwrapi.dll` and the
//! frame module into a running game, then prints once a second what the
//! module's frame callback published — whether it runs (`state`), how many
//! callbacks per second, the frame and unit rows it last read, and the
//! interval between callbacks. With `wait`, the `frame_module_wait` build
//! holds the game for 100 ms inside every callback, which shows up as the
//! interval and the game's frame progress both growing by that much: the
//! game waits for the callback.

#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    use std::time::Duration;

    use bwrapi_game::frame_module::{ModuleStatus, module_state};
    use bwrapi_game::{GameStatus, inject, inject_module};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pid) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("usage: module_status <pid> [wait] [seconds]");
        return ExitCode::FAILURE;
    };
    let wait = args.iter().skip(1).any(|argument| argument.eq_ignore_ascii_case("wait"));
    let seconds: u64 = args
        .iter()
        .skip(1)
        .filter_map(|argument| argument.parse::<u64>().ok())
        .next()
        .unwrap_or(10);

    let bwrapi = default_file("bwrapi.dll");
    if let Err(error) = inject(pid, &bwrapi) {
        eprintln!("inject {}: {error}", bwrapi.display());
        if let Some(status) = GameStatus::open(pid) {
            eprintln!("{status:?}");
        }
        return ExitCode::FAILURE;
    }
    let module = default_file(if wait { "frame_module_wait.dll" } else { "frame_module.dll" });
    if let Err(error) = inject_module(pid, &module, "frame_module_callwndproc") {
        eprintln!("inject module {}: {error}", module.display());
        return ExitCode::FAILURE;
    }

    let mut previous = ModuleStatus::open(pid).unwrap_or_default();
    for _ in 0..seconds {
        std::thread::sleep(Duration::from_secs(1));
        let Some(status) = ModuleStatus::open(pid) else {
            eprintln!("module status block is gone");
            return ExitCode::FAILURE;
        };
        let ms = |qpc: u64| match status.qpc_frequency {
            0 => 0.0,
            frequency => qpc as f64 * 1000.0 / frequency as f64,
        };
        let state = match status.state {
            module_state::RUNNING => "running",
            module_state::NO_ABI => "no bwrapi.dll",
            _ => "starting",
        };
        let frame = match status.last_game_frame {
            u32::MAX => "-".to_owned(),
            frame => frame.to_string(),
        };
        println!(
            "state={} callbacks/s={} frame={} unit_rows={} interval_ms={:.1} max_interval_ms={:.1} wait_ms={}",
            state,
            status.callbacks.saturating_sub(previous.callbacks),
            frame,
            status.last_unit_rows,
            ms(status.last_interval_qpc),
            ms(status.max_interval_qpc),
            status.wait_ms,
        );
        previous = status;
    }
    ExitCode::SUCCESS
}

/// A file of the build next to this executable, or in the target directory
/// above `examples/`.
#[cfg(windows)]
fn default_file(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let directory = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let beside = directory.join(name);
    if beside.is_file() {
        return beside;
    }
    directory.parent().map(|target| target.join(name)).unwrap_or(beside)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("module_status runs on Windows only");
}
