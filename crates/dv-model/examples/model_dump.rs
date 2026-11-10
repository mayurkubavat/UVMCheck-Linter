//! Prints the class model of a design in dv-model's canonical dump form,
//! built natively (default) or through slang (`--slang`), for diffing:
//!
//!     cargo run --release -p dv-model --features slang --example model_dump -- [--slang] [--types] [--bodies] <compile options>
use dv_model::{Design, DumpOptions};

fn main() {
    let mut opts = DumpOptions::default();
    let mut slang = false;
    let mut args = Vec::new();
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--slang" => slang = true,
            "--types" => opts.types = true,
            "--bodies" => opts.bodies = true,
            _ => args.push(a),
        }
    }
    let design = if slang { Design::load(&args) } else { dv_model::load(&args) };
    match design {
        Ok(d) => print!("{}", d.dump(opts)),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
