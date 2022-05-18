use cosmwasm_std::{StdError, OverflowError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid Amount")]
    InvalidAmount {},

    #[error("Not Enough Balance Available to Withdraw")]
    NotEnoughAvailableBalance {},

    #[error("Not Enough Funds Available to Withdraw")]
    NotEnoughAvailableFunds
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
