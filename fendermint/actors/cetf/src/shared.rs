use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use num_derive::FromPrimitive;

pub type BlockHeight = u64;
pub type Tag = [u8; 32];

pub const CETF_ACTOR_NAME: &str = "cetf";

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct EnqueueTagParams {
    pub tag: [u8; 32],
}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ClearTagParams {
    pub height: u64,
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = frc42_dispatch::method_hash!("Constructor"),
    EnqueueTag = frc42_dispatch::method_hash!("EnqueueTag"),
    ClearTag = frc42_dispatch::method_hash!("ClearTag"),
}
