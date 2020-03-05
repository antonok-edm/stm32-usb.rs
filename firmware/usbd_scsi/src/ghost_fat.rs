use packing::{
    Packed,
    PackedSize,
};

use crate::{
    BlockDevice,
    BlockDeviceError,
};

use uf2::Block as Uf2Block;

use core::ops::RangeInclusive;

#[allow(unused_imports)]
use crate::logging::*;

// 64kb left for bootloader since logging is huge
// TODO: Needs to be configurable, can this be passed in from a linker section maybe?
const UF2_FLASH_START: u32 = 0x08010000;

const BLOCK_SIZE: usize = 512;
const UF2_BLOCK_SIZE: usize = 256;
const UF2_BLOCKS_PER_FAT_BLOCK: u32 = (BLOCK_SIZE / UF2_BLOCK_SIZE) as u32;
const NUM_FAT_BLOCKS: u32 = 8000;
const RESERVED_SECTORS: u32 = 1;
const ROOT_DIR_SECTORS: u32 = 4;
const SECTORS_PER_FAT: u32 = (NUM_FAT_BLOCKS * 2 + 511) / 512;
const START_FAT0: u32 = RESERVED_SECTORS;
const START_FAT1: u32 = START_FAT0 + SECTORS_PER_FAT;
const START_ROOTDIR: u32 = START_FAT1 + SECTORS_PER_FAT;
const START_CLUSTERS: u32 = START_ROOTDIR + ROOT_DIR_SECTORS;
const UF2_SIZE: u32 = 0x10000 * 2;
const UF2_SECTORS: u32 = UF2_SIZE / (BLOCK_SIZE as u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    /// Indicates a request attempted to erase an address that would potentially cause 
    /// data loss because the address doesn't align with a page boundary
    UnsafeErase,

    /// Hardware didn't behave as expected, unrecoverable
    HardwareError,

    /// Reading back value after write returned an unexpected value
    WriteError,

    /// Reading back after erase indicated the erase failed - stm32 datasheet implies this is possible
    /// but doesn't say under what circumstances. Is the flash knackered if this happens?
    EraseError,

    /// Page address is invalid or out of range
    InvalidAddress,
}

pub trait Flash {
    // Return the page size in bytes
    fn page_size(&self) -> u32;

    fn address_range(&self) -> RangeInclusive<u32>;

    // Return a mutable ref to the persistent page buffer
    fn page_buffer(&mut self) -> &mut [u8];

    // Return the last read page address
    fn current_page(&self) -> &Option<u32>;

    // Unlock the flash for erasing/writing
    fn unlock_flash(&mut self) -> Result<(), FlashError>;

    // Lock the flash to prevent erasing/writing
    fn lock_flash(&mut self) -> Result<(), FlashError>;

    // Is the flash busy?
    fn is_operation_pending(&self) -> bool;

    // Wait until is_operation_pending is false, no timeout implemented
    fn busy_wait(&self) {
        while self.is_operation_pending() {}
    }

    // Erase the page at the given address. Don't check if erase is necessary, that's done at a higher level
    fn erase_page(&mut self, page_address: u32) -> Result<(), FlashError>;

    // Check if the page is empty
    fn is_page_erased(&mut self, page_address: u32) -> bool;

    // Read a whole page into page buffer
    fn read_page(&mut self, page_address: u32) -> Result<(), FlashError>;

    /// Write a whole page from page buffer
    ///
    /// Implementor should probably read each half-word (or whatever the flash write size is) and
    /// compare it to the data being written before writing to reduce flash aging.
    fn write_page(&mut self) -> Result<(), FlashError>;

    /// Gets the page address for a given address
    fn page_address(&self, address: u32) -> u32 {
        address & !(self.page_size() - 1)
    }

    /// Save the current page to flash. Erase and write only if necessary.
    fn flush_page(&mut self) -> Result<(), FlashError>;

    /// Write the provided bytes to flash at the provided address
    ///
    /// Each touched page will be read into a buffer and flushed back to flash when the next page
    /// is modified or at the end of the function. Page erasing is only done if data is found that
    /// is different to what is currently on flash and can't be ANDed onto flash without an erase. 
    /// i.e. Flash of 0x12FF and 0x1234 COULD be written without an erase.
    fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), FlashError> {
        let start_page = self.page_address(address);
        let end_page = self.page_address(address + bytes.len() as u32 - 1);
        let page_size = self.page_size() as usize;

        for page in (start_page..=end_page).step_by(page_size) {
            if let Some(cp) = self.current_page() {
                // If there's a page in the buffer and it's not the current one, flush it
                if *cp != page {
                    self.flush_page()?;
                    self.read_page(page)?;
                }
            } else {
                self.read_page(page)?;
            }

            if page < address {
                let offset = (address - page) as usize;
                let count = (page_size - offset).min(bytes.len());
                self.page_buffer()[offset..(offset+count)].copy_from_slice(&bytes[..count]);
            } else {
                let offset = (page - address) as usize;
                let count = (bytes.len() - offset).min(page_size);
                self.page_buffer()[..count].copy_from_slice(&bytes[offset..(offset + count)]);
            }
        }
        // Flush the last page 
        self.flush_page()?;

        Ok(())
    }

    /// Read bytes from the provided address
    fn read_bytes(&self, address: u32, bytes: &mut [u8]) -> Result<(), FlashError>;
}

#[derive(Clone, Copy, Default, Packed)]
#[packed(little_endian, lsb0)]
pub struct DirectoryEntry {    
    #[pkd(7, 0, 0, 10)]
    pub name: [u8; 11],
    /*
        pub name: [u8; 8],
        pub ext: [u8; 3],
    */
    #[pkd(7, 0, 11, 11)]
    pub attrs: u8,

    #[pkd(7, 0, 12, 12)]
    _reserved: u8,

    #[pkd(7, 0, 13, 13)]
    pub create_time_fine: u8,

    #[pkd(7, 0, 14, 15)]
    pub create_time: u16,

    #[pkd(7, 0, 16, 17)]
    pub create_date: u16,
    
    #[pkd(7, 0, 18, 19)]
    pub last_access_date: u16,
    
    #[pkd(7, 0, 20, 21)]
    pub high_start_cluster: u16,
    
    #[pkd(7, 0, 22, 23)]
    pub update_time: u16,
    
    #[pkd(7, 0, 24, 25)]
    pub update_date: u16,
    
    #[pkd(7, 0, 26, 27)]
    pub start_cluster: u16,
    
    #[pkd(7, 0, 28, 31)]
    pub size: u32,
}


pub enum FatFileContent {
    Static([u8; 255]),
    Uf2,
}

pub struct FatFile {
    pub name: [u8; 11],
    pub content: FatFileContent,
}

const ASCII_SPACE: u8 = 0x20;

impl FatFile {
    fn with_content<N: AsRef<[u8]>, T: AsRef<[u8]>>(name_: N, content_: T) -> Self {
        let mut name = [0; 11];
        let mut content = [0; 255];

        let bytes = name_.as_ref();
        let l = bytes.len().min(name.len());
        name[..l].copy_from_slice(&bytes[..l]);
        for b in name[l..].iter_mut() {
            *b = ASCII_SPACE
        }

        let bytes = content_.as_ref();
        let l = bytes.len().min(content.len());
        content[..l].copy_from_slice(&bytes[..l]);
        for b in content[l..].iter_mut() {
            *b = ASCII_SPACE
        }

        Self {
            name,
            content: FatFileContent::Static(content),
        }
    }
}

const UF2_INDEX: usize = 2;

pub fn fat_files() -> [FatFile; 3] {
    let info = FatFile::with_content("INFO_UF2TXT", "UF2 Bootloader 1.2.3\r\nModel: BluePill\r\nBoard-ID: xyz_123\r\n");
    let index = FatFile::with_content("INDEX   HTM", "<!doctype html>\n<html><body><script>\nlocation.replace(INDEX_URL);\n</script></body></html>\n");

    let mut name = [ASCII_SPACE; 11];
    name.copy_from_slice("CURRENT UF2".as_bytes());

    let current_uf2 = FatFile {
        name,
        content: FatFileContent::Uf2,
    };
    
    [info, index, current_uf2]
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Packed)]
#[packed(little_endian, lsb0)]
pub struct FatBootBlock {
    #[pkd(7, 0, 0, 2)]
    pub jump_instruction: [u8; 3],

    #[pkd(7, 0, 3, 10)]
    pub oem_info: [u8; 8],
    
    #[pkd(7, 0, 11, 12)]
    pub sector_size: u16,
    
    #[pkd(7, 0, 13, 13)]
    pub sectors_per_cluster: u8,
    
    #[pkd(7, 0, 14, 15)]
    pub reserved_sectors: u16,
    
    #[pkd(7, 0, 16, 16)]
    pub fat_copies: u8,
    
    #[pkd(7, 0, 17, 18)]
    pub root_directory_entries: u16,
    
    #[pkd(7, 0, 19, 20)]
    pub total_sectors16: u16,
    
    #[pkd(7, 0, 21, 21)]
    pub media_descriptor: u8,
    
    #[pkd(7, 0, 22, 23)]
    pub sectors_per_fat: u16,
    
    #[pkd(7, 0, 24, 25)]
    pub sectors_per_track: u16,
    
    #[pkd(7, 0, 26, 27)]
    pub heads: u16,
    
    #[pkd(7, 0, 28, 31)]
    pub hidden_sectors: u32,
    
    #[pkd(7, 0, 32, 35)]
    pub total_sectors32: u32,
    
    #[pkd(7, 0, 36, 36)]
    pub physical_drive_num: u8,
    
    #[pkd(7, 0, 37, 37)]
    _reserved: u8,
    
    #[pkd(7, 0, 38, 38)]
    pub extended_boot_sig: u8,
    
    #[pkd(7, 0, 39, 42)]
    pub volume_serial_number: u32,
    
    #[pkd(7, 0, 43, 53)]
    pub volume_label: [u8; 11],
    
    #[pkd(7, 0, 54, 61)]
    pub filesystem_identifier: [u8; 8],
}

pub fn fat_boot_block() -> FatBootBlock {
    const RESERVED_SECTORS: u16 = 1;
    const ROOT_DIR_SECTORS: u16 = 4;
    const NUM_FAT_BLOCKS: u16 = 8000;
    const SECTORS_PER_FAT: u16 = (NUM_FAT_BLOCKS * 2 + 511) / 512;
    let mut fat = FatBootBlock {
        jump_instruction: [0xEB, 0x3C, 0x90],
        oem_info: [0x20; 8],
        sector_size: 512,
        sectors_per_cluster: 1,
        reserved_sectors: RESERVED_SECTORS,
        fat_copies: 2,
        root_directory_entries: (ROOT_DIR_SECTORS * 512 / 32),
        total_sectors16: NUM_FAT_BLOCKS - 2,
        media_descriptor: 0xF8,
        sectors_per_fat: SECTORS_PER_FAT,
        sectors_per_track: 1,
        heads: 1,
        hidden_sectors: 0,
        total_sectors32: NUM_FAT_BLOCKS as u32 - 1,
        physical_drive_num: 0,
        _reserved: 0,
        extended_boot_sig: 0x29,
        volume_serial_number: 0x00420042,
        volume_label: [0x20; 11],
        filesystem_identifier: [0x20; 8],
    };
    fat.oem_info[..7].copy_from_slice("UF2 UF2".as_bytes());
    fat.volume_label[..8].copy_from_slice("BLUEPILL".as_bytes());
    fat.filesystem_identifier[..5].copy_from_slice("FAT16".as_bytes());

    fat
}


/// # Dummy fat implementation that provides a [UF2 bootloader](https://github.com/microsoft/uf2)
pub struct GhostFat<F: Flash> {
    fat_boot_block: FatBootBlock,
    fat_files: [FatFile; 3],
    flash: F,
}

impl<F: Flash> BlockDevice for GhostFat<F> {
    const BLOCK_BYTES: usize = BLOCK_SIZE;
    fn read_block(&self, lba: u32, block: &mut [u8]) -> Result<(), BlockDeviceError> {
        self.read_block(lba, block);
        Ok(())
    }
    fn write_block(&mut self, lba: u32, block: &[u8]) -> Result<(), BlockDeviceError> {
        self.write_block(lba, block);
        Ok(())
    }
    fn max_lba(&self) -> u32 {
        NUM_FAT_BLOCKS - 1
    }
}


impl<F: Flash> GhostFat<F> {
    pub fn new(flash: F) -> Self {
        GhostFat {
            fat_boot_block: fat_boot_block(),
            fat_files: fat_files(),
            flash,
        }
    }

    pub fn read_block(&self, lba: u32, block: &mut [u8]) {
        assert_eq!(block.len(), BLOCK_SIZE);

        info!("GhostFAT reading block: 0x{:X?}", lba);

        // Clear the buffer since we're sending all of it
        for b in block.iter_mut() { *b = 0 }
   
        if lba == 0 {
            // Block 0 is the fat boot block
            self.fat_boot_block.pack(&mut block[..FatBootBlock::BYTES]).unwrap();
            block[510] = 0x55;
            block[511] = 0xAA;
        } else if lba < START_ROOTDIR {
            let mut section_index = lba - START_FAT0;

            if section_index >= SECTORS_PER_FAT {
                section_index -= SECTORS_PER_FAT;
            }

            if section_index == 0 {
                block[0] = 0xF0;
                for i in 1..(self.fat_files.len() * 2 + 4) {
                    block[i] = 0xFF;
                }
            }
         
            let uf2_first_sector = self.fat_files.len() + 1;
            let uf2_last_sector = uf2_first_sector + UF2_SECTORS as usize - 1;
            
            for i in 0..256_usize {
                let v = section_index as usize * 256 + i;
                let j = 2 * i;
                if v >= uf2_first_sector && v < uf2_last_sector {
                    block[j+0] = (((v + 1) >> 0) & 0xFF) as u8;
                    block[j+1] = (((v + 1) >> 8) & 0xFF) as u8;
                } else if v == uf2_last_sector {
                    block[j+0] = 0xFF;
                    block[j+1] = 0xFF;
                }
            }            
        } else if lba < START_CLUSTERS {
            let section_index = lba - START_ROOTDIR;            
            if section_index == 0 {
                let mut dir = DirectoryEntry::default();
                dir.name.copy_from_slice(&self.fat_boot_block.volume_label);
                dir.attrs = 0x28;
                let len = DirectoryEntry::BYTES;
                dir.pack(&mut block[..len]).unwrap();
                dir.attrs = 0;
                for (i, info) in self.fat_files.iter().enumerate() {
                    dir.name.copy_from_slice(&info.name);
                    dir.start_cluster = i as u16 + 2;
                    dir.size = match info.content {
                        FatFileContent::Static(content) => content.len() as u32,
                        FatFileContent::Uf2 => {
                            let address_range = self.flash.address_range();
                            (address_range.end() - address_range.start()) * UF2_BLOCKS_PER_FAT_BLOCK
                        },
                    };
                    let start = (i+1) * len;
                    dir.pack(&mut block[start..(start+len)]).unwrap();
                }
            }
        } else {
            let section_index = (lba - START_CLUSTERS) as usize;

            if section_index < UF2_INDEX {//self.fat_files.len() {
                let info = &self.fat_files[section_index];
                if let FatFileContent::Static(content) = &info.content {
                    block[..content.len()].copy_from_slice(content);
                }
            } else {
                //UF2
                info!("UF2: {}", section_index);

                let uf2_block_num = (section_index - 2) as u32;
                let address = UF2_FLASH_START + uf2_block_num * (UF2_BLOCK_SIZE as u32);
                let address_range = self.flash.address_range();
                if address_range.contains(&address) && address_range.contains(&(address + UF2_BLOCK_SIZE as u32)) {
                    let mut uf2_block = Uf2Block::default();

                    // Copy the 256 bytes into the data array
                    self.flash.read_bytes(address, &mut uf2_block.data[..UF2_BLOCK_SIZE]).unwrap();

                    // Update the header data
                    uf2_block.payload_size = UF2_BLOCK_SIZE as u32;
                    uf2_block.target_address = address;
                    uf2_block.block_number = uf2_block_num;
                    uf2_block.number_of_blocks = 
                        (address_range.end() - address_range.start()) / UF2_BLOCK_SIZE as u32;

                    Packed::pack(&uf2_block, block).unwrap();
                }                
            }
        }
    }

    pub fn write_block(&mut self, lba: u32, block: &[u8]) {
        info!("GhostFAT writing block: 0x{:X?}", lba);

        if lba < (START_CLUSTERS + self.fat_files.len() as u32) {
            info!("    GhostFAT skipping non-UF2 area");
            return;
        }

        if block.len() < Uf2Block::BYTES {
            warn!("    GhostFAT attempt to write to UF2 area with < 512 byte block");
            return;
        }
        assert_eq!(block.len(), Uf2Block::BYTES);
        
        let uf2 = if let Ok(uf2) = Uf2Block::parse(block) {
            uf2
        } else {
            warn!("   GhostFAT failed to parse as UF2");
            return;
        };

        if !uf2_family_is_correct(&uf2) {
            warn!("   GhostFAT UF2 family id was wrong");
            return;
        }


        info!("   GhostFAT writing {} bytes of UF2 block at 0x{:X?}", uf2.payload_size, uf2.target_address);          
        self.flash.write_bytes(uf2.target_address, &uf2.data[..uf2.payload_size as usize]).unwrap();
    }
}

fn uf2_family_is_correct(_uf2: &Uf2Block) -> bool {
    true
}