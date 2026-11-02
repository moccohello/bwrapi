//! `status <pid> [dll] [seconds]`: injects `bwrapi.dll` into a running game
//! and prints the frame callback and render hook status once a second.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::Duration;

    use bwrapi_game::GameStatus;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pid) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("usage: status <pid> [dll] [seconds]");
        return ExitCode::FAILURE;
    };
    let dll = args.get(1).map(PathBuf::from).unwrap_or_else(default_dll);
    let seconds: u64 = args.get(2).and_then(|value| value.parse().ok()).unwrap_or(10);

    if let Err(error) = bwrapi_game::inject(pid, &dll) {
        eprintln!("inject {}: {error}", dll.display());
        if let Some(status) = GameStatus::open(pid) {
            eprintln!("{status:?}");
        }
        return ExitCode::FAILURE;
    }
    let mut previous = GameStatus::open(pid).unwrap_or_default();
    for _ in 0..seconds {
        std::thread::sleep(Duration::from_secs(1));
        let Some(status) = GameStatus::open(pid) else {
            eprintln!("status block is gone");
            return ExitCode::FAILURE;
        };
        let ms = |qpc: u64| match status.qpc_frequency {
            0 => 0.0,
            frequency => qpc as f64 * 1000.0 / frequency as f64,
        };
        let frame = match status.last_game_frame {
            u32::MAX => "-".to_owned(),
            frame => frame.to_string(),
        };
        println!(
            "callbacks/s={} frame={} skipped={} same_thread={} max_gap_ms={:.1} max_latency_ms={:.2} \
             renders/s={} render_thread_same={} render_us={:.1} max_render_us={:.1}",
            status.callbacks.saturating_sub(previous.callbacks),
            frame,
            status.skipped_frames,
            status.install_thread_id == status.step_thread_id,
            ms(status.max_gap_qpc),
            ms(status.max_latency_qpc),
            status.renders.saturating_sub(previous.renders),
            status.render_thread_id == status.install_thread_id,
            ms(status.last_render_qpc) * 1000.0,
            ms(status.max_render_qpc) * 1000.0,
        );
        previous = status;
    }
    ExitCode::SUCCESS
}

/// `bwrapi.dll` next to this executable, or in the target directory above
/// `examples/`.
#[cfg(windows)]
fn default_dll() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let directory = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let beside = directory.join("bwrapi.dll");
    if beside.is_file() {
        return beside;
    }
    directory.parent().map(|target| target.join("bwrapi.dll")).unwrap_or(beside)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("status runs on Windows only");
}
