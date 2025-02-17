# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.6.0] - 2025-02-17

Most of the changes pertain to Merlin components.

### Fixes

* Docstrings are handled more consistently
* Nested conditionals are handled more accurately
* Variables are tracked more accurately
* Various minor fixes

### New Features

* Macro expansions resolve `IF` and `DO` in many cases
* Tracking of macro dependencies to maximum allowed depth

### New Behaviors

* `a2kit verify` returns an error if handed an empty string
* Duplicate labels in a macro trigger a warning rather than multiple errors
* Conditional macro definitions are handled differently
    - they are always discouraged by a warning
    - they will be dimmed if the conditional evaluates false

## [3.5.1] - 2025-01-09

### Fixes

* Fix error in disassembly with label substitution
* Account for label substitution in unit testing

## [3.5.0] - 2024-12-29

### Fixes

* Eliminate possible panics due to missing WOZ tracks
* Allow for repeated address fields when solving nibble tracks
* CLI multi-sector `put` strictly seeks in angle order
    - this matters for tracks with repeated sector addresses
    - non-contiguous sequences can be used to resolve ambiguities

### New Behaviors

* CLI multi-sector `put` may resolve differently (see above)

## [3.4.0] - 2024-11-17

### Fixes

* Correct an issue with the assember's addressing-mode matcher
* Correct some issues that could come up in disassembly
* Various forms of `RUN` can end an Applesoft relocation flow

### New Features

* Support converting data to code in Merlin source files

### New Behaviors

* Prevent excessively long workspace scans
    - Limit the directory count and recursion depth
    - Skip `build`, `node_modules`, and `target` directories

## [3.3.3] - 2024-10-26

### Fixes

* Detokenizing from RAM image works for Integer programs ending at $C000
* More bounds checking of RAM images

## [3.3.2] - 2024-10-06

### Fixes

* Servers advertise the `executeCommand` capability (required by Neovim)
* Applesoft server complies with elevated minification request
* Prevent certain truncations in level 3 minifier
* Refine blank line handling in a few places

## [3.3.1] - 2024-09-29

### Fixes

* Consistent case control settings in `lang` module

## [3.3.0] - 2024-09-21

### New Features

* Language server arguments to ease integration with Neovim
* Subcommand to generate shell completions

### Removed Features

* Shell completions will not be packaged with each release since they can be generated on the fly

## [3.2.0] - 2024-09-15

### Fixes

* Repairs to BASIC renumbering
    - updates all references
    - moves can handle blank lines

### New Behaviors

* The `LineNumberTool` trait is deprecated, use the `Renumber` trait instead.
* Improved LSP symbol manipulations and checks
* LSP declarations, definitions, and references do not overlap.

## [3.1.0] - 2024-09-08

### New Features

* Handles Teledisk 1.x in addition to 2.x
* Responds to a client's workspace symbols request
* Make some use of backup FATs

### Fixes

* Formatting preserves blank lines
* Correct a bug in rename symbol
* Correct a bug in address hovers
* Always zero high word of cluster1 for FAT12/16

## [3.0.2] - 2024-08-24

### Fixes

* Better handling of Merlin conditionals
* Disassembler identifies out of bounds branches as data
* Automatic unpacking uses both file system hints and actual data

## [3.0.1] - 2024-08-18

### Fixes

* More complete handling of Merlin folding ranges
* ProDOS and FAT glob patterns are automatically prefixed when necessary
* CP/M generic catalog includes user numbers when there are users other than user 0
* Disk server write operations will actually write
* Fix an issue with the head map in IMD and TD0 images

## [3.0.0] - 2024-08-11

### New Features

* Language servers for Applesoft, Integer BASIC, and Merlin
    - these can be used to support any editor that implements the LSP
* `a2kit verify` performs a much deeper language analysis
* Disassembler for 6502, 65c02, and 65816
* Limited assembler for Merlin 8, 16, 16+, 32
* new CLI subcommands
    - `mget` and `mput` for efficient multi-file handling
    - `pack` and `unpack` for direct manipulation of file images
    - `glob` allows you to glob (search) any solvable disk image
* new CLI subcommand options
    - `get -t auto` will automatically select an unpacking strategy
    - `catalog --generic` will produce the same columns no matter the FS
    - some subcommands have `--indent` option to control JSON formatting
* Applesoft optionally accepts extended CALL syntax
* Integer optionally accepts immediate mode commands
* Renumbering optionally allows for movement of lines

### Fixes

* Eliminate some possible panics
* Better handling of 16 bit Merlin syntax
* Better ProDOS Y2K handling
* Better handling of CP/M files with no extension

### New Behaviors

Most of the JSON outputs will now default to a minified format.  This is optimal when `a2kit` is being called as a subprocess or library.  Users of the raw CLI can recover pretty formatting using the `--indent` option.

### Breaking Changes

CLI scripts written for v2 should still work with v3, *unless* a user's JSON parser is checking the *length* of certain arrays, or checking for unknown keys.

Users of the `a2kit` library crate will have to consider the following before upgrading to v3:

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
* Function calls that will need to be revised
    - `a2kit_macro::DiskStruct::from_bytes`
    - `a2kit_macro::DiskStruct::update_from_bytes`
    - `img::DiskImage::from_bytes`
    - `fs::<>::Disk::from_img`
    - `crate::try_img`
* Sector sizes are included in track solutions as a fourth element in the CHS map subarrays
    - The outward facing `geometry` subcommand still uses the `chs_map` key, so user parsers will still succeed if they ignore extra array elements.
* `FileImage::desequence` clears the chunk map before updating chunks, while `Records::update_fimg` does so optionally
