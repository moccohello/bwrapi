//! `bwrapi-inject <pid> <dll> [export]`: loads a DLL into a running game of
//! this program's pointer width. It ships beside each `bwrapi.dll`, so an
//! injector of the other width (a 64-bit launcher and an x86 game) runs the
//! one beside the game's DLLs. Without `export`, `dll` is `bwrapi.dll` itself
//! and its frame hook is installed. With `export`, `dll` is a module that
//! uses the public ABI and installs from that window-hook export on the game
//! thread; the `bwrapi.dll` beside it is injected first when the game does
//! not run one yet. For the in-game BWAPI server:
//! `bwrapi-inject <pid> <dir>\bwrapi_bwapi.dll bwrapi_bwapi_callwndproc`.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::path::PathBuf;
    use std::process::ExitCode;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(pid), Some(dll)) = (args.first().and_then(|value| value.parse::<u32>().ok()), args.get(1)) else {
        eprintln!("usage: bwrapi-inject <pid> <dll> [export]");
        return ExitCode::FAILURE;
    };
    let dll = PathBuf::from(dll);
    // A game of the other width would hand the work back to this same file.
    let result = match bwrapi_game::matches_game(pid) {
        Ok(true) => match args.get(2) {
            Some(export) => bwrapi_game::inject_module(pid, &dll, export),
            None => bwrapi_game::inject(pid, &dll),
        },
        Ok(false) => Err(format!(
            "game {pid} is not an {} process: use the bwrapi-inject.exe in the folder for the game's bit width",
            if cfg!(target_pointer_width = "64") { "x64" } else { "x86" }
        )),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => {
            println!("injected {} into {pid}", dll.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("inject {}: {error}", dll.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("bwrapi-inject runs on Windows only");
}
