mod arena;
mod classes;
mod enums;
mod unions;
mod name;
mod size;
mod convert;
mod write;

use pdb;

pub use pdb::{PrimitiveKind, ClassKind, Variant as EnumValue};

pub use self::arena::*;
pub use self::classes::*;
pub use self::enums::*;
pub use self::unions::*;
pub use self::name::*;
pub use self::size::*;
pub use self::convert::*;
pub use self::write::*;

// TODO: what happens with recursive classes?

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

