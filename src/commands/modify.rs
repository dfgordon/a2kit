use clap;
use crate::STDRESULT;
const RCH: &str = "unreachable was reached";

pub fn mkdir(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img,fmt.as_ref())?;
    disk.create(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn delete(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.delete(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn rename(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let name = cmd.get_one::<String>("name").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.rename(&path_in_img,&name)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn retype(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let typ = cmd.get_one::<String>("type").expect(RCH);
    let aux = cmd.get_one::<String>("aux").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.retype(&path_in_img,&typ,&aux)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn protect(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let password = cmd.get_one::<String>("password").expect(RCH);
    let read = cmd.get_flag("read");
    let write = cmd.get_flag("write");
    let delete = cmd.get_flag("delete");
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.protect(path_in_img,password,read,write,delete)?;
    return crate::save_img(&mut disk,path_to_img);
}

pub fn unprotect(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.unprotect(path_in_img)?;
    return crate::save_img(&mut disk,path_to_img);
}

pub fn lock(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.lock(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}

pub fn unlock(cmd: &clap::ArgMatches) -> STDRESULT {
    let path_to_img = cmd.get_one::<String>("dimg").expect(RCH);
    let path_in_img = cmd.get_one::<String>("file").expect(RCH);
    let fmt = super::get_fmt(cmd)?;
    let mut disk = crate::create_fs_from_file_pro(&path_to_img, fmt.as_ref())?;
    disk.unlock(&path_in_img)?;
    return crate::save_img(&mut disk,&path_to_img);
}
