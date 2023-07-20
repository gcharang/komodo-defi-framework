//! Contains rpc data layer structures that are not ready to become a part of the mm2_rpc::data module
//!
//! *Note: it's expected that the following data types will be moved to mm2_rpc::data when mm2 is refactored to be able to handle them*
//!

mod activation;
mod swaps;
mod trading;

pub(crate) use activation::*;
pub(crate) use swaps::*;
pub(crate) use trading::*;

//TODO: @rozhkovdmitrii
