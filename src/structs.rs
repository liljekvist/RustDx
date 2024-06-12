pub(crate) mod camera;
pub(crate) mod texture;
pub(crate) mod vertex;
pub(crate) mod camera_uniform;
pub(crate) mod instance;

#[allow(dead_code)]
#[repr(C)]
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CompareFunction {
    Undefined = 0,
    Never = 1,
    Less = 2,
    Equal = 3,
    LessEqual = 4,
    Greater = 5,
    NotEqual = 6,
    GreaterEqual = 7,
    Always = 8,
}
