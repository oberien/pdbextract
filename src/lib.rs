#![feature(nll)]
#![feature(match_default_bindings)]

extern crate pdb;
extern crate regex;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate failure;
extern crate multimap;

pub mod ir;
