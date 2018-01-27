mod arena;
mod classes;
mod enums;
mod unions;
mod name;
mod convert;
mod size;

use pdb;

pub use pdb::{PrimitiveKind, ClassKind, Variant as EnumValue};

pub use self::arena::*;
pub use self::classes::*;
pub use self::enums::*;
pub use self::unions::*;
pub use self::name::*;
pub use self::size::*;
pub use self::convert::*;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "pdb error: {}", err)]
    Pdb {
        err: pdb::Error,
    },
    #[fail(display = "not yet implemented: {}", cause)]
    Unimplemented {
        cause: String,
    }
}

impl From<pdb::Error> for Error {
    fn from(err: pdb::Error) -> Self {
        Error::Pdb { err }
    }
}

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

