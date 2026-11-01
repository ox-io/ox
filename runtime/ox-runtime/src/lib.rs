#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeInfo {
    pub name: &'static str,
    pub version: &'static str,
}

pub const RUNTIME: RuntimeInfo = RuntimeInfo {
    name: "Ox",
    version: env!("CARGO_PKG_VERSION"),
};
