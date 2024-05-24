use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use num_derive::FromPrimitive;

pub const CETF_ACTOR_NAME: &str = "cetf";

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct EnqueueTagParams {
    pub tag: [u8; 32],
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    EnqueueTag = frc42_dispatch::method_hash!("EnqueueTag"),
}
