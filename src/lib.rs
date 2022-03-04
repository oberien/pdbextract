use std::fs::File;
use std::path::Path;
use pdb::PDB;
use crate::ir::{Arena, Converter};

pub mod ir;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("pdb error: {0}")]
    Pdb(#[from] pdb::Error),
    #[error("not yet implemented: {0}")]
    Unimplemented(String),
    #[error("error during writing: {0}")]
    WriteError(#[from] std::io::Error),
}

pub type Result<T> = ::std::result::Result<T, Error>;

// TODO: what happens with recursive classes?

pub fn parse<P: AsRef<Path>>(path: P) -> Result<Arena> {
    let mut arena = Arena::new();
    let file = File::open(path)?;
    let mut pdb = PDB::open(file)?;
    let mut info = pdb.type_information()?;
    let mut converter = Converter::new(&mut info, &mut arena)?;
    converter.populate()?;
    Ok(arena)
}
