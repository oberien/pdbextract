extern crate pdbextract;

use std::env;
use std::io;

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

    let arena = read(&file).unwrap();
    let mut writer = Writer::new(io::stdout(), &arena);
    for name in structs {
        writer.write_type(arena[&name]).unwrap();
    }
}
