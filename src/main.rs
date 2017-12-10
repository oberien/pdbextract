extern crate pdbextract;

use std::env;
use std::io;

use pdbextract::PdbExtract;

enum State {
    Struct,
    Ignore,
    Replace,
}

fn main() {
    let mut args = env::args().skip(1);
    let file = args.next().unwrap();
    let mut extract = PdbExtract::new(file);
    let mut state = State::Struct;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--struct" => {
                state = State::Struct;
                continue
            }
            "--ignore" => {
                state = State::Ignore;
                continue;
            }
            "--replace" => {
                state = State::Replace;
                continue;
            }
            _ => {
                match state {
                    State::Struct => extract.add_struct(arg),
                    State::Ignore => extract.ignore_struct(arg),
                    State::Replace => extract.replace(&arg, args.next().unwrap()),
                };
            }
        }
    }
    if let Err(e) = extract.write(io::stdout()) {
        eprintln!("backtrace: {:?}", e.backtrace());
        eprintln!("error: {:?}", e);
        let mut e = e.cause();
        while let Some(cause) = e.cause() {
            eprintln!("caused by: {:?}", cause);
            e = cause;
        }
    }
}
