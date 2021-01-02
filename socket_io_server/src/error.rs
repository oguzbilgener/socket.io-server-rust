use std::{error, fmt};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EmitError {
    EmptyEventName,
    BroadcastError
}

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmitError::EmptyEventName => {
                write!(f, "Event name cannot be empty")
            },
            EmitError::BroadcastError => {
                write!(f, "Broadcast failed")
            }
        }
    }
}

impl error::Error for EmitError {}


#[derive(Debug, Copy, Clone, PartialEq)]
pub struct BroadcastError;

impl fmt::Display for BroadcastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Broadcast failed")
    }
}

impl error::Error for BroadcastError {}

impl From<BroadcastError> for EmitError {
    fn from(_: BroadcastError) -> Self {
        EmitError::BroadcastError
    }
}