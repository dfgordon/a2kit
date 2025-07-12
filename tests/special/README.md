# Tests of Special Disk Formats

## Local Only

The images needed for these tests are not included in the public repository to avoid licensing conflict.  The tests are written to pass with a warning in the event that the images are not found.

## Background

This folder contains disks with special formats, and for each one, a corresponding disk format
specification.  The latter is a JSON file that contains the information needed to interpret the special format.

These tools make no particular effort to defeat copy protection.
Rather, they allow cross-developers to create a special format.
Unlocking file systems of classic game disks is an enjoyable side effect that is also useful for testing.

## Castle Wolfenstein

This is a modified DOS 3.2 disk that boots on DOS 3.3 hardware.  For unlocking the file system it is sufficient to double the sector id on tracks 3-34.  The following additional information may be useful.

Track 0 is a hybrid track:
* sectors 0-9 and 11-12 are normal 5&3 sectors
* sector 10 is a pristine (see note) 5&3 sector, but...
* there is a 6&2 boot sector in the space normally reserved for the sector 10 data field.

This image has the following quarter-track usage:
* tracks 0 and 3 occupy 3 quarter tracks each, n, n+1/4, n+1/2
* all other tracks occupy 4 quarter tracks each, n-1/4, n, n+1/4, n+1/2

Note: A pristine 5&3 sector is one with no data field, for when DOS 3.2 formats a track, no data fields are created.  If someone tries to read a sector before it is written, zeroes are returned.  As a result, something can be hidden in a pristine sector's data region, and the track will still read normally.

## Ultima IV

This is a modified DOS 3.3 disk.  The markers follow a pattern that can be handled with some simple masking (this does not suffice to format a disk the same way, however).  This also involves a minor nibble transformation.  There is a half-track at the end that is not handled, but it seems this has no effect on retrieving files.

## Ultima V

This is a modified ProDOS disk.  Tracks 3-34 have 17 added to the sector id, and the address epilog is modified.  The system program is `DINKEYDOS` rather than `PRODOS`, but this appears to have no effect on our ability to manipulate files.