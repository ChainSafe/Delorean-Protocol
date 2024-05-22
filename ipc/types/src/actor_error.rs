// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_ipld_encoding::de::DeserializeOwned;
use fvm_ipld_encoding::ipld_block::IpldBlock;
use std::fmt::Display;

use fvm_shared::error::ExitCode;
use thiserror::Error;

/// The error type returned by actor method calls.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("ActorError(exit_code: {exit_code:?}, msg: {msg})")]
pub struct ActorError {
    /// The exit code for this invocation.
    /// Codes less than `FIRST_USER_EXIT_CODE` are prohibited and will be overwritten by the VM.
    exit_code: ExitCode,
    /// Optional exit data
    data: Option<IpldBlock>,
    /// Message for debugging purposes,
    msg: String,
}

impl ActorError {
    /// Creates a new ActorError. This method does not check that the code is in the
    /// range of valid actor abort codes.
    pub fn unchecked(code: ExitCode, msg: String) -> Self {
        Self {
            exit_code: code,
            msg,
            data: None,
        }
    }

    pub fn unchecked_with_data(code: ExitCode, msg: String, data: Option<IpldBlock>) -> Self {
        Self {
            exit_code: code,
            msg,
            data,
        }
    }

    /// Creates a new ActorError. This method checks if the exit code is within the allowed range,
    /// and automatically converts it into a user code.
    pub fn checked(code: ExitCode, msg: String, data: Option<IpldBlock>) -> Self {
        let exit_code = match code {
            // This means the called actor did something wrong. We can't "make up" a
            // reasonable exit code.
            ExitCode::SYS_MISSING_RETURN
            | ExitCode::SYS_ILLEGAL_INSTRUCTION
            | ExitCode::SYS_ILLEGAL_EXIT_CODE => ExitCode::USR_UNSPECIFIED,
            // We don't expect any other system errors.
            code if code.is_system_error() => ExitCode::USR_ASSERTION_FAILED,
            // Otherwise, pass it through.
            code => code,
        };
        Self {
            exit_code,
            msg,
            data,
        }
    }

    pub fn illegal_argument(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_ILLEGAL_ARGUMENT,
            msg,
            data: None,
        }
    }
    pub fn not_found(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_NOT_FOUND,
            msg,
            data: None,
        }
    }
    pub fn forbidden(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_FORBIDDEN,
            msg,
            data: None,
        }
    }
    pub fn insufficient_funds(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_INSUFFICIENT_FUNDS,
            msg,
            data: None,
        }
    }
    pub fn illegal_state(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_ILLEGAL_STATE,
            msg,
            data: None,
        }
    }
    pub fn serialization(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_SERIALIZATION,
            msg,
            data: None,
        }
    }
    pub fn unhandled_message(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_UNHANDLED_MESSAGE,
            msg,
            data: None,
        }
    }
    pub fn unspecified(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_UNSPECIFIED,
            msg,
            data: None,
        }
    }
    pub fn assertion_failed(msg: String) -> Self {
        Self {
            exit_code: ExitCode::USR_ASSERTION_FAILED,
            msg,
            data: None,
        }
    }

    /// Returns the exit code of the error.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }

    /// Error message of the actor error.
    pub fn msg(&self) -> &str {
        &self.msg
    }

    /// Extracts the optional associated data without copying.
    pub fn take_data(&mut self) -> Option<IpldBlock> {
        std::mem::take(&mut self.data)
    }

    /// Prefix error message with a string message.
    pub fn wrap(mut self, msg: impl AsRef<str>) -> Self {
        self.msg = format!("{}: {}", msg.as_ref(), self.msg);
        self
    }
}

/// Converts a raw encoding error into an ErrSerialization.
impl From<fvm_ipld_encoding::Error> for ActorError {
    fn from(e: fvm_ipld_encoding::Error) -> Self {
        Self {
            exit_code: ExitCode::USR_SERIALIZATION,
            msg: e.to_string(),
            data: None,
        }
    }
}

/// Converts an actor deletion error into an actor error with the appropriate exit code. This
/// facilitates propagation.
#[cfg(feature = "fil-actor")]
impl From<fvm_sdk::error::ActorDeleteError> for ActorError {
    fn from(e: fvm_sdk::error::ActorDeleteError) -> Self {
        Self {
            exit_code: ExitCode::USR_ILLEGAL_ARGUMENT,
            msg: e.to_string(),
            data: None,
        }
    }
}

/// Converts a state-read error into an an actor error with the appropriate exit code (illegal actor).
/// This facilitates propagation.
#[cfg(feature = "fil-actor")]
impl From<fvm_sdk::error::StateReadError> for ActorError {
    fn from(e: fvm_sdk::error::StateReadError) -> Self {
        Self {
            exit_code: ExitCode::USR_ILLEGAL_STATE,
            data: None,
            msg: e.to_string(),
        }
    }
}

/// Converts a state update error into an an actor error with the appropriate exit code.
/// This facilitates propagation.
#[cfg(feature = "fil-actor")]
impl From<fvm_sdk::error::StateUpdateError> for ActorError {
    fn from(e: fvm_sdk::error::StateUpdateError) -> Self {
        Self {
            exit_code: match e {
                fvm_sdk::error::StateUpdateError::ActorDeleted => ExitCode::USR_ILLEGAL_STATE,
                fvm_sdk::error::StateUpdateError::ReadOnly => ExitCode::USR_READ_ONLY,
            },
            data: None,
            msg: e.to_string(),
        }
    }
}

/// Convenience macro for generating Actor Errors
#[macro_export]
macro_rules! actor_error {
    // Error with only one stringable expression
    ( $code:ident; $msg:expr ) => { $crate::ActorError::$code($msg.to_string()) };

    // String with positional arguments
    ( $code:ident; $msg:literal $(, $ex:expr)+ ) => {
        $crate::ActorError::$code(format!($msg, $($ex,)*))
    };

    // Error with only one stringable expression, with comma separator
    ( $code:ident, $msg:expr ) => { $crate::actor_error!($code; $msg) };

    // String with positional arguments, with comma separator
    ( $code:ident, $msg:literal $(, $ex:expr)+ ) => {
        $crate::actor_error!($code; $msg $(, $ex)*)
    };
}

// Adds context to an actor error's descriptive message.
pub trait ActorContext<T> {
    fn context<C>(self, context: C) -> Result<T, ActorError>
    where
        C: Display + 'static;

    fn with_context<C, F>(self, f: F) -> Result<T, ActorError>
    where
        C: Display + 'static,
        F: FnOnce() -> C;
}

impl<T> ActorContext<T> for Result<T, ActorError> {
    fn context<C>(self, context: C) -> Result<T, ActorError>
    where
        C: Display + 'static,
    {
        self.map_err(|mut err| {
            err.msg = format!("{}: {}", context, err.msg);
            err
        })
    }

    fn with_context<C, F>(self, f: F) -> Result<T, ActorError>
    where
        C: Display + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|mut err| {
            err.msg = format!("{}: {}", f(), err.msg);
            err
        })
    }
}

// Adapts a target into an actor error.
pub trait AsActorError<T>: Sized {
    fn exit_code(self, code: ExitCode) -> Result<T, ActorError>;

    fn context_code<C>(self, code: ExitCode, context: C) -> Result<T, ActorError>
    where
        C: Display + 'static;

    fn with_context_code<C, F>(self, code: ExitCode, f: F) -> Result<T, ActorError>
    where
        C: Display + 'static,
        F: FnOnce() -> C;
}

// Note: E should be std::error::Error, revert to this after anyhow:Error is no longer used.
impl<T, E: Display> AsActorError<T> for Result<T, E> {
    fn exit_code(self, code: ExitCode) -> Result<T, ActorError> {
        self.map_err(|err| ActorError {
            exit_code: code,
            msg: err.to_string(),
            data: None,
        })
    }

    fn context_code<C>(self, code: ExitCode, context: C) -> Result<T, ActorError>
    where
        C: Display + 'static,
    {
        self.map_err(|err| ActorError {
            exit_code: code,
            msg: format!("{context}: {err}"),
            data: None,
        })
    }

    fn with_context_code<C, F>(self, code: ExitCode, f: F) -> Result<T, ActorError>
    where
        C: Display + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|err| ActorError {
            exit_code: code,
            msg: format!("{}: {}", f(), err),
            data: None,
        })
    }
}

impl<T> AsActorError<T> for Option<T> {
    fn exit_code(self, code: ExitCode) -> Result<T, ActorError> {
        self.ok_or_else(|| ActorError {
            exit_code: code,
            msg: "None".to_string(),
            data: None,
        })
    }

    fn context_code<C>(self, code: ExitCode, context: C) -> Result<T, ActorError>
    where
        C: Display + 'static,
    {
        self.ok_or_else(|| ActorError {
            exit_code: code,
            msg: context.to_string(),
            data: None,
        })
    }

    fn with_context_code<C, F>(self, code: ExitCode, f: F) -> Result<T, ActorError>
    where
        C: Display + 'static,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| ActorError {
            exit_code: code,
            msg: f().to_string(),
            data: None,
        })
    }
}

pub fn deserialize_block<T>(ret: Option<IpldBlock>) -> Result<T, ActorError>
where
    T: DeserializeOwned,
{
    ret.context_code(
        ExitCode::USR_ASSERTION_FAILED,
        "return expected".to_string(),
    )?
    .deserialize()
    .exit_code(ExitCode::USR_SERIALIZATION)
}
