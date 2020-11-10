#![forbid(unsafe_code)]
extern crate base64;
extern crate nom;

// This library implements the engine.io protocol revision 3.
pub const PROTOCOL: u8 = 3;

pub mod binary;
pub mod packet;
pub mod string;
pub mod decoder;
