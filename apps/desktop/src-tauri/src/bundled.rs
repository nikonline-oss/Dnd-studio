//! System plugins shipped with the app (packed from `plugins/` by `build.rs`, ADR-0023).

/// `(id, package bytes)` of every bundled plugin.
pub const BUNDLED: &[(&str, &[u8])] = &[
    (
        "srd5",
        include_bytes!(concat!(env!("OUT_DIR"), "/system-srd5.dndplugin")),
    ),
    (
        "generic-d6",
        include_bytes!(concat!(env!("OUT_DIR"), "/system-generic-d6.dndplugin")),
    ),
];

/// The package of a bundled plugin.
#[must_use]
pub fn package(id: &str) -> Option<&'static [u8]> {
    BUNDLED
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, bytes)| *bytes)
}
