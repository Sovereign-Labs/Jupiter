use sovereign_sdk::core::traits::Address;

#[derive(Debug, PartialEq, Clone)]
pub struct CelestiaAddress(pub Vec<u8>);

impl AsRef<[u8]> for CelestiaAddress {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl Address for CelestiaAddress {}

impl<'a> TryFrom<&'a [u8]> for CelestiaAddress {
    type Error = ();

    fn try_from(value: &'a [u8]) -> Result<Self, ()> {
        Ok(Self(value.to_vec()))
    }
}
