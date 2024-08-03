# 3.0.0

(in progress)

## Major Features

* Language servers for Applesoft, Integer BASIC, and Merlin
    - these can be used to support any editor that implements the LSP
* `a2kit verify` performs a much deeper language analysis
* Disassembler for 6502, 65c02, and 65816
* Limited assembler for Merlin 8, 16, 16+, 32
* deliberating: pass multiple files through pipe at once
* deliberating: revised pipeline stages

## Other Updates

* Applesoft optionally accepts extended CALL syntax
* Integer optionally accepts immediate mode commands
* Better renumbering
* Better handling of 16 bit Merlin syntax
* Better ProDOS Y2K handling
* Beter handling of CP/M files with no extension
* `catalog` has `--generic` option for easy parsing with any file system
* deliberating: `get` has `-t auto` to automatically select a decoding strategy

## Internals

* New trait functions `catalog_to_vec`
* deliberating: New trait functions `load_any`, `fimg_file_data`, `fimg_load_address`

## Breaking

* Enumerations that are extended: `ItemType`, some `Error` enumerations scoped by module
* Syntax tree walker is different
    - `Visit` and `WalkerChoice` are replaced by `Navigate` and `Navigation`
* FileImage 2.1.0 is the default, this simply adds the `accessed` key which can be used by FAT file systems
* Functions with updated return values
    - `a2kit_macro::DiskStruct::from_bytes`
    - `a2kit_macro::DiskStruct::update_from_bytes`
    - `img::DiskImage::from_bytes`
    - `fs::<>::Disk::from_img`
    - `crate::try_img`
* Functions with updated argument types
    - `a2kit_macro::DiskStruct::from_bytes`
    - `a2kit_macro::DiskStruct::update_from_bytes`
    - `img::DiskImage::from_bytes`
* Sector sizes are included in track solutions as a fourth element in the CHS map subarrays
    - The outward facing `geometry` subcommand still uses the `chs_map` key, so user parsers will still succeed if they ignore extra array elements.
