use packing::Packed;

use crate::scsi::enums::{
    CapacityDescriptorCode,
};

#[derive(Clone, Copy, Eq, PartialEq, Debug, Packed)]
#[packed(big_endian, lsb0)]
pub struct CapacityListHeader {
    #[pkd(7, 0, 3, 3)]
    pub capacity_list_length: u8,
}

impl Default for CapacityListHeader {
    fn default() -> Self {
        Self {
            capacity_list_length: 8,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Packed)]
#[packed(big_endian, lsb0)]
pub struct CurrentMaximumCapacityDescriptor {
    #[pkd(7, 0, 0, 3)]
    pub number_of_blocks: u32,

    #[pkd(1, 0, 4, 4)]
    pub descriptor_code: CapacityDescriptorCode,

    #[pkd(7, 0, 5, 7)]
    pub block_length: u32,
}

impl CurrentMaximumCapacityDescriptor {
    pub fn with_number_of_blocks(number_of_blocks: u32) -> Self {
        Self {
            number_of_blocks,
            descriptor_code: Default::default(),
            block_length: 512,
        }
    }
}
