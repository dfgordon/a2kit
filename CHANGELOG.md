# 3.0.0

(in progress)

## Major Features

* Language servers for Applesoft, Integer BASIC, and Merlin
    - these can be used to support any editor that implements the LSP
* `a2kit verify` performs a much deeper language analysis
* Disassembler for 6502, 65c02, and 65816
* Limited assembler for Merlin 8, 16, 16+, 32
* new CLI subcommands `mget` and `mput` for efficient multi-file handling
* new CLI subcommands `pack` and `unpack` for direct manipulation of file images

## Other Updates

* You can glob (search) a disk image using `a2kit glob`
* Applesoft optionally accepts extended CALL syntax
* Integer optionally accepts immediate mode commands
* Better renumbering
* Better handling of 16 bit Merlin syntax
* Better ProDOS Y2K handling
* Better handling of CP/M files with no extension
* `catalog` has `--generic` option for easy parsing with any file system
* `get` has `-t auto` to automatically select a decoding strategy
* More control over JSON formatting
* Eliminate some possible panics

## Breaking Changes

Breaking changes are all at the level of the library.  CLI scripts written for v2 should still work with v3, *unless* a user's JSON parser is checking the *length* of certain arrays.

* Extracting files from a disk image works differently
    - The `FileImage` object handles packing or unpacking various data types, while `DiskFS` works only with `FileImage`.
    - `DiskFS` has convenience functions similar to the old `save`, `bsave`, etc., but some arguments and return values have changed.
    - `TextConversion` trait replaces `TextEncoder` trait
* Enumerations that are extended: `ItemType`, some `Error` enumerations scoped by module
* Syntax tree walker is different
    - `Visit` and `WalkerChoice` are replaced by `Navigate` and `Navigation`
* FileImage 2.1.0 is the default, this adds two root level keys
    - `full_path` key which is mainly useful for `mput`
    - `accessed` key which can be used by FAT file systems
* Functions calls that will need to be reviewed
    - `a2kit_macro::DiskStruct::from_bytes`
    - `a2kit_macro::DiskStruct::update_from_bytes`
    - `img::DiskImage::from_bytes`
    - `fs::<>::Disk::from_img`
    - `crate::try_img`
* Sector sizes are included in track solutions as a fourth element in the CHS map subarrays
    - The outward facing `geometry` subcommand still uses the `chs_map` key, so user parsers will still succeed if they ignore extra array elements.
* `FileImage::desequence` clears the chunk map before updating chunks, while `Records::update_fimg` does so optionally
