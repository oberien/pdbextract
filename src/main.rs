extern crate pdbextract;

use std::env;
use std::io;

use pdbextract::PdbExtract;
use pdbextract::ir::*;

enum State {
    Struct,
    Ignore,
    Replace,
}

fn main() {
    let mut args = env::args().skip(1);
    let file = args.next().unwrap();
    let mut structs = Vec::new();
    let mut ignore = Vec::new();
    let mut replace = Vec::new();
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
                    State::Struct => structs.push(arg),
                    State::Ignore => ignore.push(arg),
                    State::Replace => replace.push((arg, args.next().unwrap())),
                };
            }
        }
    }
//    let mut extract = PdbExtract::new(file);
//    structs.into_iter().for_each(|s| { extract.add_struct(s); });
//    ignore.into_iter().for_each(|i| { extract.ignore_struct(i); });
//    replace.into_iter().for_each(|(p, r)| { extract.replace(&p, r); });
//    if let Err(e) = extract.write(io::stdout()) {
//        eprintln!("backtrace: {:?}", e.backtrace());
//        eprintln!("error: {:?}", e);
//        let mut e = e.cause();
//        while let Some(cause) = e.cause() {
//            eprintln!("caused by: {:?}", cause);
//            e = cause;
//        }
//    }
    let arena = read(&file).unwrap();
    let mut writer = Writer::new(io::stdout(), &arena);
    for name in structs {
        writer.write_type(arena[&name]).unwrap();
    }
}
