use clap;
use crate::STDRESULT;
const RCH: &str = "unreachable was reached";

pub fn mkdir(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file(&path_to_img,fmt.as_ref())?;
    disk.create(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn delete(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file(&path_to_img, fmt.as_ref())?;
    disk.delete(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn rename(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let name = cmd.get_one::<String>("name").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file(&path_to_img, fmt.as_ref())?;
    disk.rename(&path_in_img,&name)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn retype(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let typ = cmd.get_one::<String>("type").expect(RCH);
    let aux = cmd.get_one::<String>("aux").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file(&path_to_img, fmt.as_ref())?;
    disk.retype(&path_in_img,&typ,&aux)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn access(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let password = match cmd.get_one::<String>("password") {
        Some(s) => Some(s.as_str()),
        None => None
    };
    let mut permissions = crate::fs::Attributes::new();
    if cmd.get_flag("read") {
        permissions = permissions.read(true);
    }
    if cmd.get_flag("write") {
        permissions = permissions.write(true);
    }
    if cmd.get_flag("delete") {
        permissions = permissions.destroy(true);
    }
    if cmd.get_flag("rename") {
        permissions = permissions.rename(true);
    }
    if cmd.get_flag("no-read") {
        permissions = permissions.read(false);
    }
    if cmd.get_flag("no-write") {
        permissions = permissions.write(false);
    }
    if cmd.get_flag("no-delete") {
        permissions = permissions.destroy(false);
    }
    if cmd.get_flag("no-rename") {
        permissions = permissions.rename(false);
    }
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file(&path_to_img, fmt.as_ref())?;
    disk.set_attrib(path_in_img,permissions,password)?;
    return crate::save_img(&mut disk,path_to_img);
}
