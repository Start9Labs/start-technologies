use startos::registry::asset::RegistryAsset;
pub use startos::registry::os::index::OsVersionInfo;
pub use startos::registry::os::SIG_CONTEXT;
use startos::sign::commitment::blake3::Blake3Commitment;

pub fn asset_for_platform<'a>(
    info: &'a OsVersionInfo,
    platform: &str,
) -> Option<&'a RegistryAsset<Blake3Commitment>> {
    info.squashfs
        .get(platform)
        .or_else(|| info.img.get(platform))
        .or_else(|| info.iso.get(platform))
}
