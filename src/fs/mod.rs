//! # File System Module
//! 
//! This is a container for file system modules.  File system modules handle
//! interactions with directories and files.  They are largely independent
//! of the `img` module, because they retain their own version of the disk data
//! in a convenient form.  N.b. this means you have
//! to explicitly transfer save changes to the original disk image if you want them
//! to be permanent.
//! 
//! File systems are represented by the `DiskFS` trait.

pub mod dos33;
pub mod prodos;
pub mod pascal;