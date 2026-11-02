//! Writes the generated C header and C#/Python declarations,
//! `bwrapi-gen <out-dir>`, or the C ABI reference, `bwrapi-gen --doc <file>`
//! (`docs/C-ABI.md` of the repository; see `generate::REGENERATE`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, file] if flag == "--doc" => {
            std::fs::write(file, bwrapi::generate::markdown()).expect("write");
        }
        [out] => {
            let out = std::path::PathBuf::from(out);
            std::fs::create_dir_all(&out).expect("out dir");
            for (name, text) in [
                ("bwrapi.h", bwrapi::generate::c_header()),
                ("Native.g.cs", bwrapi::generate::csharp()),
                ("_native.py", bwrapi::generate::python()),
            ] {
                std::fs::write(out.join(name), text).expect("write");
            }
        }
        _ => {
            eprintln!("usage: bwrapi-gen <out-dir> | bwrapi-gen --doc <file>");
            std::process::exit(2);
        }
    }
}
