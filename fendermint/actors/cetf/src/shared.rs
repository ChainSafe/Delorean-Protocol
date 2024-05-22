use num_derive::FromPrimitive;

pub const CETF_ACTOR_NAME: &str = "enquque_tag";

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Invoke = frc42_dispatch::method_hash!("Invoke"),
}
