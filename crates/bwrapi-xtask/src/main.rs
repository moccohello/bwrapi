use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const BWAPI_REPOSITORY_URL: &str = "https://github.com/bwapi/bwapi.git";
const DOTNET_EXAMPLE_PROJECT: &str = "dotnet/examples/ObserverQuickstart/ObserverQuickstart.csproj";
const DOTNET_CLIENT_PROJECT: &str = "dotnet/BwrApi.Client/BwrApi.Client.csproj";
/// Text files that ship beside the binaries: the user notes of the C++ zip
/// and the third-party notice for BWAPI and the Rust crates in bwrapi.dll.
const PACKAGE_TEXT_ROOT: &str = "host/package";
const NOTICE_FILE: &str = "THIRD_PARTY_NOTICES.md";
/// Name stem of the folder and zip for C++ AIModule and BWAPI client users.
const CPP_BUNDLE_STEM: &str = "bwrapi-windows";
/// C examples of the ABI that ship in the C++ bundle's `examples/`.
const C_EXAMPLE_ROOT: &str = "crates/ffi/examples";
const C_EXAMPLES: &[&str] = &["read_units.c", "submit_move.c"];

/// One pointer width of the game the Windows build serves: `bwrapi.dll` with
/// its injector from cargo, the in-game server from a host CMake configure.
/// Shipped files of a width sit in a folder named after it.
struct Arch {
    name: &'static str,
    /// Cargo target triple; `None` builds for the x64 Windows host.
    triple: Option<&'static str>,
    /// Visual Studio platform of the host configure.
    platform: &'static str,
    /// Host build directory under `<out>`.
    host_dir: &'static str,
}

const ARCHES: [Arch; 2] = [
    Arch { name: "x64", triple: None, platform: "x64", host_dir: "host" },
    Arch { name: "x86", triple: Some("i686-pc-windows-msvc"), platform: "Win32", host_dir: "host-x86" },
];

impl Arch {
    /// Cargo release directory of this width under the cargo target `target`.
    fn cargo_release(&self, target: &Path) -> PathBuf {
        match self.triple {
            Some(triple) => target.join(triple).join("release"),
            None => target.join("release"),
        }
    }
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    root: PathBuf,
    out: PathBuf,
    bwapi_source: Option<PathBuf>,
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    let options = parse_options(&args[1..])?;
    match command {
        "build" => build(&options),
        "test" => test(&options),
        "package" => package(&options),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unsupported xtask command: {other}")),
    }
}

fn print_help() {
    println!("BWRAPI tasks");
    println!("  cargo xtask build     cargo release (x64 and x86 bwrapi.dll), host CMake for both widths and dotnet build (cargo build only on Linux)");
    println!("  cargo xtask test      cargo tests, pytest, host ctest, dotnet build (cargo and pytest on Linux)");
    println!("  cargo xtask package   builds, then writes the Python wheel, the NuGet package and the C++ zip (x64/ and x86/ native files) to <out>/dist (Windows)");
    println!("Options:");
    println!("  --out <path>          output root shared by build, test and package (default <repo>/build)");
    println!("  --bwapi-source <path> local BWAPI 4.4 upstream mirror replacing the pinned git fetch");
    println!("  --root <path>         repository root override");
}

// The xtask binary is built from the repository, so its compile-time manifest
// directory is the default root, whatever the working directory is.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the xtask crate lives at <repository>/crates/bwrapi-xtask")
        .to_path_buf()
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    // std::path::absolute resolves a relative path against the working
    // directory without requiring it to exist and without a Windows \\?\
    // device prefix.
    let resolve = |path: &str| -> Result<PathBuf, String> {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            Ok(path)
        } else {
            std::path::absolute(&path).map_err(|error| format!("{}: {error}", path.display()))
        }
    };
    let mut root = repository_root();
    let mut out: Option<PathBuf> = None;
    let mut bwapi_source: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let value = match args[index].as_str() {
            "--root" | "--out" | "--bwapi-source" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    format!("missing value for {}", args[index])
                })?;
                index += 2;
                value
            }
            other => return Err(format!("unsupported option: {other}")),
        };
        match args[index - 2].as_str() {
            "--root" => root = resolve(value)?,
            "--out" => out = Some(resolve(value)?),
            "--bwapi-source" => bwapi_source = Some(resolve(value)?),
            _ => unreachable!("option names were matched above"),
        }
    }
    if !root.is_dir() {
        return Err(format!(
            "invalid repository root is not a directory: {}",
            root.display()
        ));
    }
    Ok(Options {
        out: out.unwrap_or_else(|| root.join("build")),
        root,
        bwapi_source,
    })
}

fn is_windows() -> bool {
    env::consts::OS == "windows"
}

fn build(options: &Options) -> Result<(), String> {
    let cargo_target = options.out.join("cargo");
    if is_windows() {
        build_abi(options, &cargo_target)?;
        build_host(options)?;
        dotnet_build(options)?;
    } else {
        cargo_step(&options.root, &["build", "--workspace"], &cargo_target, "cargo build")?;
        build_abi(options, &cargo_target)?;
    }
    Ok(())
}

/// Builds the public ABI library with its injector (on Windows for x64 and
/// x86) and writes its generated C header and C#/Python declarations to
/// `<out>/bindings`.
fn build_abi(options: &Options, target: &Path) -> Result<(), String> {
    let bindings = options.out.join("bindings");
    let bindings = bindings.to_str().ok_or("bindings path must be UTF-8")?;
    cargo_step(&options.root, &["build", "--release", "-p", "bwrapi-ffi"], target, "ABI library build")?;
    if is_windows() {
        for triple in ARCHES.iter().filter_map(|arch| arch.triple) {
            cargo_step(
                &options.root,
                &["build", "--release", "-p", "bwrapi-ffi", "--target", triple],
                target,
                "ABI library build",
            )?;
        }
    }
    cargo_step(
        &options.root,
        &["run", "--release", "-p", "bwrapi-ffi", "--bin", "bwrapi-gen", "--", bindings],
        target,
        "ABI bindings generation",
    )
}

/// Configures and builds the in-game BWAPI server of each width under
/// `<out>/<host_dir>`, and the x86 loaders with the x64 one under
/// `<out>/host`. The ABI header and libraries must be built first.
fn build_host(options: &Options) -> Result<PathBuf, String> {
    let cmake = windows_cmake()?;
    for arch in &ARCHES {
        cmake_configure(&cmake, options, arch)?;
        ensure_svnrev_header(&upstream_root(options))?;
        cmake_build(&cmake, &options.out.join(arch.host_dir), &[])?;
    }
    Ok(cmake)
}

fn test(options: &Options) -> Result<(), String> {
    let cargo_target = options.out.join("cargo");
    cargo_step(
        &options.root,
        &["test", "--workspace", "--lib", "--tests"],
        &cargo_target,
        "cargo test",
    )?;
    if is_windows() {
        build_abi(options, &cargo_target)?;
        let cmake = build_host(options)?;
        let host_build = options.out.join("host");
        for name in ctest_target_names(&host_build)? {
            cmake_build(&cmake, &host_build, &[&name])?;
        }
        ctest_run(&ctest_executable(&cmake), &host_build)?;
    }
    let upstream = ensure_bwapi_source(options)?;
    let upstream_option = format!(
        "--bwapi-source={}",
        upstream.to_string_lossy()
    );
    python_step(
        &options.root.join("python"),
        &[
            "-m",
            "pytest",
            "-q",
            "-p",
            "no:cacheprovider",
            // The equals form keeps pytest from treating the upstream path as
            // a testpath before conftest.py registers the option.
            &upstream_option,
        ],
        "pytest",
    )?;
    if is_windows() {
        dotnet_build(options)?;
    }
    Ok(())
}

/// Builds what ships and writes it to `<out>/dist`: the Python wheel, the
/// NuGet package and the folder and zip for C++ users. It builds the ABI and
/// the server first, so it also runs on a fresh output directory.
fn package(options: &Options) -> Result<(), String> {
    if !is_windows() {
        return Err("cargo xtask package builds the Windows server and loaders; run it on Windows".to_owned());
    }
    let cargo_target = options.out.join("cargo");
    build_abi(options, &cargo_target)?;
    build_host(options)?;
    install_python_native(options)?;

    let dist = options.out.join("dist");
    fs::create_dir_all(&dist).map_err(|error| format!("{}: {error}", dist.display()))?;
    // setuptools copies package files into python/build and never removes
    // ones the package no longer has, which would put them into the wheel.
    remove_directory_if_present(&options.root.join("python").join("build"))?;
    let python_source = options.root.join("python").to_string_lossy().into_owned();
    let dist_text = dist.to_string_lossy().into_owned();
    python_step(
        &options.root,
        &[
            "-m",
            "pip",
            "wheel",
            &python_source,
            "--no-deps",
            "--wheel-dir",
            &dist_text,
        ],
        "python wheel build",
    )?;
    run_step(
        Command::new("dotnet")
            .arg("pack")
            .arg(options.root.join(DOTNET_CLIENT_PROJECT))
            .args(["--configuration", "Release", "--output"])
            .arg(&dist)
            .arg(format!("-p:BwrApiCargoTarget={}", cargo_target.display())),
        "dotnet pack",
    )?;
    write_cpp_bundle(options, &dist)?;
    println!("packages written: {}", dist.display());
    Ok(())
}

fn run_step(command: &mut Command, description: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("start {description}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{description} failed: {status}"))
    }
}

fn cargo_step(root: &Path, args: &[&str], target: &Path, description: &str) -> Result<(), String> {
    run_step(
        Command::new("cargo")
            .args(args)
            .current_dir(root)
            .env("CARGO_TARGET_DIR", target),
        description,
    )
}

fn python_step(directory: &Path, args: &[&str], description: &str) -> Result<(), String> {
    let program = if is_windows() { "python" } else { "python3" };
    run_step(
        Command::new(program).args(args).current_dir(directory),
        description,
    )
}

/// Builds the .NET binding through its example, which also builds the native
/// library and the generated declarations.
fn dotnet_build(options: &Options) -> Result<(), String> {
    run_step(
        Command::new("dotnet")
            .arg("build")
            .arg(options.root.join(DOTNET_EXAMPLE_PROJECT))
            .args(["--configuration", "Release"])
            .arg(format!("-p:BwrApiCargoTarget={}", options.out.join("cargo").display())),
        ".NET build",
    )
}

fn windows_cmake() -> Result<PathBuf, String> {
    if Command::new("cmake")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return Ok(PathBuf::from("cmake"));
    }
    let Some(program_files) = env::var_os("ProgramFiles") else {
        return Err("cmake was not found on PATH and ProgramFiles is unset".to_owned());
    };
    let visual_studio = PathBuf::from(program_files).join("Microsoft Visual Studio");
    let mut found = Vec::new();
    find_cmake_executables(&visual_studio, 0, &mut found);
    found.sort();
    found
        .pop()
        .ok_or_else(|| format!("Windows CMake was not found under {}", visual_studio.display()))
}

fn find_cmake_executables(directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_cmake_executables(&path, depth + 1, found);
        } else if path
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(r"\microsoft\cmake\cmake\bin\cmake.exe")
        {
            found.push(path);
        }
    }
}

fn cmake_configure(cmake: &Path, options: &Options, arch: &Arch) -> Result<(), String> {
    let mut command = Command::new(cmake);
    command
        .arg("-S")
        .arg(options.root.join("host"))
        .arg("-B")
        .arg(options.out.join(arch.host_dir))
        .args(["-A", arch.platform, "-DCMAKE_SUPPRESS_REGENERATION=ON"])
        .arg(format!(
            "-DFETCHCONTENT_BASE_DIR={}",
            options.out.join("_deps").display()
        ))
        // The public ABI: `build_abi` writes the header and builds the library.
        .arg(format!(
            "-DBWRAPI_ABI_INCLUDE_DIR={}",
            options.out.join("bindings").display()
        ))
        .arg(format!(
            "-DBWRAPI_ABI_LIBRARY={}",
            arch.cargo_release(&options.out.join("cargo")).join("bwrapi.dll.lib").display()
        ));
    if let Some(mirror) = &options.bwapi_source {
        command.arg(format!("-DFETCHCONTENT_SOURCE_DIR_BWAPI={}", mirror.display()));
    }
    run_step(&mut command, "host CMake configure")
}

fn cmake_build(cmake: &Path, build_dir: &Path, targets: &[&str]) -> Result<(), String> {
    let mut command = Command::new(cmake);
    command
        .arg("--build")
        .arg(build_dir)
        .args(["--config", "Release"]);
    for target in targets {
        command.args(["--target", target]);
    }
    run_step(&mut command, "host CMake build")
}

fn ctest_executable(cmake: &Path) -> PathBuf {
    if cmake.as_os_str() == "cmake" {
        PathBuf::from("ctest.exe")
    } else {
        cmake.with_file_name("ctest.exe")
    }
}

// ctest -N hides tests whose executable has not been built yet, so the test
// set is read from the CTestTestfile.cmake manifests that configure emits.
// The first add_test argument is the test name and the second is the command,
// which this project defines as the CMake target name for every test.
fn ctest_target_names(build_dir: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut queue = vec![build_dir.to_path_buf()];
    while let Some(directory) = queue.pop() {
        let manifest = directory.join("CTestTestfile.cmake");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("subdirs(") {
                for entry in rest.trim_end_matches(')').split(';') {
                    let subdirectory = entry.trim().trim_matches('"');
                    if !subdirectory.is_empty() {
                        queue.push(directory.join(subdirectory));
                    }
                }
            } else if let Some(rest) = line.strip_prefix("add_test(") {
                if rest.contains("NOT_AVAILABLE") {
                    continue;
                }
                let name = match rest.find("[=[") {
                    Some(open) => rest[open + 3..]
                        .find("]=]")
                        .map(|close| &rest[open + 3..open + 3 + close])
                        .unwrap_or_default(),
                    None => rest.split('"').nth(1).unwrap_or_default(),
                };
                if !name.is_empty() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Err(format!(
            "host CMake registered no CTest targets under {}",
            build_dir.display()
        ));
    }
    Ok(names)
}

fn ctest_run(ctest: &Path, build_dir: &Path) -> Result<(), String> {
    run_step(
        Command::new(ctest)
            .arg("--test-dir")
            .arg(build_dir)
            .args(["-C", "Release", "--output-on-failure"]),
        "host ctest",
    )
}

fn ensure_bwapi_source(options: &Options) -> Result<PathBuf, String> {
    if let Some(mirror) = &options.bwapi_source {
        if !upstream_include_dir(mirror).is_dir() {
            return Err(format!(
                "BWAPI upstream mirror does not contain bwapi/include: {}",
                mirror.display()
            ));
        }
        ensure_svnrev_header(mirror)?;
        return Ok(mirror.clone());
    }
    let target = options.out.join("_deps").join("bwapi-src");
    if upstream_include_dir(&target).is_dir() {
        ensure_svnrev_header(&target)?;
        return Ok(target);
    }
    if target.exists() && !target.join(".git").is_dir() {
        return Err(format!(
            "BWAPI upstream location is not a git checkout: {}",
            target.display()
        ));
    }
    if !target.exists() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        run_step(
            Command::new("git")
                .args(["clone", BWAPI_REPOSITORY_URL])
                .arg(&target),
            "BWAPI upstream clone",
        )?;
    }
    let commit = pinned_bwapi_commit(&options.root)?;
    run_step(
        Command::new("git")
            .arg("-C")
            .arg(&target)
            .args(["checkout", "--detach", &commit]),
        "BWAPI upstream checkout",
    )?;
    if !upstream_include_dir(&target).is_dir() {
        return Err(format!(
            "BWAPI upstream checkout does not contain bwapi/include: {}",
            target.display()
        ));
    }
    ensure_svnrev_header(&target)?;
    Ok(target)
}

fn upstream_root(options: &Options) -> PathBuf {
    options
        .bwapi_source
        .clone()
        .unwrap_or_else(|| options.out.join("_deps").join("bwapi-src"))
}

// The upstream repository does not track include/svnrev.h; its build stamps
// the file with the git commit count plus the upstream migration offset that
// revisionUpdate.vbs documents. The tournament loader and the Python
// upstream-reading tests both consume it, so xtask materializes the same
// file once the pinned source is present.
fn ensure_svnrev_header(source: &Path) -> Result<(), String> {
    let header = source.join("bwapi").join("include").join("svnrev.h");
    if header.is_file() {
        return Ok(());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-list", "HEAD", "--count"])
        .output()
        .map_err(|error| format!("start BWAPI revision count: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "BWAPI revision count failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let count: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|error| format!("BWAPI revision count is not a number: {error}"))?;
    let revision = 2383 + count;
    fs::write(
        &header,
        format!("#pragma once\nstatic const int SVN_REV = {revision};\n\n#include \"starcraftver.h\"\n"),
    )
    .map_err(|error| format!("{}: {error}", header.display()))
}

fn upstream_include_dir(root: &Path) -> PathBuf {
    root.join("bwapi").join("include")
}

fn pinned_bwapi_commit(root: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(root.join("host").join("CMakeLists.txt"))
        .map_err(|error| format!("read host CMakeLists.txt: {error}"))?;
    let Some(after_tag) = manifest.split("GIT_TAG").nth(1) else {
        return Err("host CMakeLists.txt does not pin a BWAPI GIT_TAG".to_owned());
    };
    let commit: String = after_tag
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_hexdigit())
        .collect();
    if commit.len() == 40 {
        Ok(commit)
    } else {
        Err(format!(
            "host CMakeLists.txt BWAPI GIT_TAG is not a commit hash: {commit}"
        ))
    }
}

// Files of the host builds: the server with the bwrapi.dll and injector the
// build copies beside it, in each width's build directory, and the x86
// loaders the x64 configure builds.
const HOST_SERVER_FILES: &[&str] = &["Release/bwrapi_bwapi.dll", "Release/bwrapi.dll", "Release/bwrapi-inject.exe"];
const HOST_LOADERS: &[&str] = &[
    "legacy44-loader/bwrapi-legacy44-loader.exe",
    "legacy44-tournament-loader/bwrapi-legacy44-tournament-loader.exe",
];

fn host_output(options: &Options, host_dir: &str, relative: &str) -> Result<PathBuf, String> {
    let source = options.out.join(host_dir).join(relative);
    if source.is_file() {
        Ok(source)
    } else {
        Err(format!("{} is missing; the host build makes the server and loaders", source.display()))
    }
}

/// The native files that ship, as (source, path in the per-width layout):
/// `x64/` and `x86/` each hold a server with its `bwrapi.dll` and injector,
/// and `x86/` also holds the loaders.
fn native_files(options: &Options) -> Result<Vec<(PathBuf, String)>, String> {
    let mut files = Vec::new();
    for arch in &ARCHES {
        for relative in HOST_SERVER_FILES {
            let source = host_output(options, arch.host_dir, relative)?;
            files.push((source, format!("{}/{}", arch.name, file_name(relative))));
        }
    }
    for relative in HOST_LOADERS {
        files.push((host_output(options, "host", relative)?, format!("x86/{}", file_name(relative))));
    }
    Ok(files)
}

fn file_name(relative: &str) -> &str {
    relative.rsplit('/').next().unwrap_or(relative)
}

// The wheel carries the per-width native files with the generated Python
// binding (ABI 1.0) and the license and notice that cover them; the launcher
// finds them under `bwrapi/bin`.
fn install_python_native(options: &Options) -> Result<(), String> {
    let package = options.root.join("python").join("bwrapi");
    copy_file(
        &options.out.join("bindings").join("_native.py"),
        &package.join("_native.py"),
    )?;
    let bin = package.join("bin");
    remove_directory_if_present(&bin)?;
    for (source, relative) in native_files(options)? {
        copy_file(&source, &bin.join(relative))?;
    }
    copy_file(&options.root.join("LICENSE"), &bin.join("LICENSE"))?;
    copy_file(
        &options.root.join(PACKAGE_TEXT_ROOT).join(NOTICE_FILE),
        &bin.join(NOTICE_FILE),
    )?;
    println!("python native files refreshed: {}", bin.display());
    Ok(())
}

/// The folder and zip for C++ users: the per-width native files (the
/// injector loads bwrapi.dll from the server's directory) with the import
/// library MSVC links bwrapi.dll through, the ABI header, the C examples and
/// the text files.
fn write_cpp_bundle(options: &Options, dist: &Path) -> Result<(), String> {
    let folder = dist.join(CPP_BUNDLE_STEM);
    remove_directory_if_present(&folder)?;
    let text = options.root.join(PACKAGE_TEXT_ROOT);
    let examples = options.root.join(C_EXAMPLE_ROOT);
    let mut files = native_files(options)?;
    for arch in &ARCHES {
        files.push((
            arch.cargo_release(&options.out.join("cargo")).join("bwrapi.dll.lib"),
            format!("{}/bwrapi.lib", arch.name),
        ));
    }
    files.extend(C_EXAMPLES.iter().map(|name| (examples.join(name), format!("examples/{name}"))));
    files.extend([
        (options.out.join("bindings").join("bwrapi.h"), "include/bwrapi.h".to_owned()),
        (text.join("README.txt"), "README.txt".to_owned()),
        (options.root.join("LICENSE"), "LICENSE".to_owned()),
        (text.join(NOTICE_FILE), NOTICE_FILE.to_owned()),
    ]);
    for (source, relative) in &files {
        copy_file(source, &folder.join(relative))?;
    }
    let zip = dist.join(format!("{CPP_BUNDLE_STEM}.zip"));
    if zip.exists() {
        fs::remove_file(&zip).map_err(|error| format!("{}: {error}", zip.display()))?;
    }
    // Windows ships bsdtar, which writes zip archives; another tar earlier on
    // PATH (Git for Windows) may not.
    let tar = env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("tar.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("tar"));
    run_step(
        Command::new(tar)
            .args(["-a", "-c", "-f"])
            .arg(&zip)
            .arg("-C")
            .arg(&folder)
            .args(["x64", "x86", "include", "examples", "README.txt", "LICENSE", NOTICE_FILE]),
        "C++ bundle zip",
    )?;
    println!("C++ bundle written: {}", zip.display());
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("package input is missing: {}", source.display()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::copy(source, destination)
        .map_err(|error| format!("{} -> {}: {error}", source.display(), destination.display()))?;
    Ok(())
}

fn remove_directory_if_present(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}
