use packing::Packed;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Packed)]
pub enum CapacityDescriptorCode {
    /// Maximum formattable capacity for this cartridge
    UnformattedMedia =0b1,
    /// Current media capacity
    FormattedMedia =0b10,
    /// Maximum formattable capacity for any cartridge
    NoCartridgeInDrive =0b11,
}
impl Default for CapacityDescriptorCode {
    fn default() -> Self {
        Self::NoCartridgeInDrive
    }
}
