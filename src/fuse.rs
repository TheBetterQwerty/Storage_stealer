use crate::github::{ FileTree, FileType, Node };
use fuser::{
    ReplyAttr, FileAttr, Filesystem, ReplyDirectory, ReplyEntry, Request, FileType as FuserFileType
};
use libc::{EIO, EISDIR, ENOENT};
use std::io::{Seek, SeekFrom, Write};
use std::time::{Duration, SystemTime};
use std::ffi::OsStr;
use crate::github::File;

const TTL: Duration = Duration::from_secs(1);

fn node_to_attr(node: &Node) -> FileAttr {
    use fuser::FileType::*;
    FileAttr {
        ino: node.ino,
        size: match &node.kind {
                FileType::File(f) => f.size,
                FileType::Dir(_) => 0,
        },
        blocks: 1,
        atime: SystemTime::now(),
        mtime: SystemTime::now(),
        ctime: SystemTime::now(),
        crtime: SystemTime::now(),
        kind: match node.kind {
            FileType::Dir(_) => Directory,
            FileType::File(_) => RegularFile
        },
        perm: match node.kind {
            FileType::Dir(_) => 0o755,
            FileType::File(_) => 0o644,
        },
        nlink: 1,
        uid: unsafe { libc::getuid() },
        gid: unsafe {libc::getuid() },
        rdev: 0,
        blksize: 512,
        flags: 0
    }
}

impl Filesystem for FileTree {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_node = match self.nodes.get(&parent) {
            Some(p) => p,
            None => return reply.error(ENOENT)
        };

        let children = match &parent_node.kind {
            FileType::Dir(children) => children,
            _ => return reply.error(ENOENT)
        };

        let name_str = match name.to_str() {
            Some(x) => x,
            None => return reply.error(ENOENT),
        };

        let child_ino = match children.get(name_str) {
            Some(&x) => x,
            None => return reply.error(ENOENT)
        };

        let child_node = &self.nodes[&child_ino];
        let attr = node_to_attr(child_node);

        reply.entry(&TTL, &attr, 0);
    }

    fn readdir(&mut self, _req: &Request<'_>, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        let node = match self.nodes.get(&ino) {
            Some(n) => n,
            None => return reply.error(ENOENT)
        };

        let children = match &node.kind {
            FileType::Dir(children) => children,
            FileType::File(_) => return reply.error(ENOENT)
        };

        let mut entries: Vec<_> = Vec::new();

        entries.push((ino, FuserFileType::Directory, ".".to_string()));

        let parent = if ino == self.root { ino } else { node.parent };
        entries.push((parent, FuserFileType::Directory, "..".into()));

        for (name, child_ino) in children {
            let child = &self.nodes[&child_ino];
            let kind = match child.kind {
                FileType::Dir(_) => FuserFileType::Directory,
                FileType::File(_) => FuserFileType::RegularFile
            };

            entries.push((*child_ino, kind, name.clone()));
        }

        for (i, (ino, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(ino, (i + 1) as i64, kind, name) {
                break;
            }
        }

        reply.ok();
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let node = match self.nodes.get(&ino) {
            Some(n) => n,
            None => return reply.error(ENOENT)
        };

        reply.attr(&TTL, &node_to_attr(node));
    }

    fn read(&mut self, _req: &Request<'_>, ino: u64, _fh: u64, offset: i64, size: u32, _flags: i32, _lock_owner: Option<u64>, reply: fuser::ReplyData) {
        let node = match self.nodes.get_mut(&ino) {
            Some(n) => n,
            None => return reply.error(ENOENT)
        };

        let file = match &node.kind {
            FileType::File(f) => f,
            FileType::Dir(_) => return reply.error(ENOENT)
        };

        let data = match &file.tmp_file {
            Some(tmp_file) => {
                std::fs::read(tmp_file).expect("error: reading file")
            },
            None => {
                let tmp_file = self.handle.block_on(async {
                   self.github.download_file(&file.name).await
                });

                std::fs::read(tmp_file.expect("File not downloaded!")).expect("error: reading file")
            },
        };

        let offset = offset as usize;
        if offset >= data.len() {
            return reply.data(&[]);
        }

        let end = std::cmp::min(offset + size as usize, data.len());
        reply.data(&data[offset..end]);
    }

    fn write(&mut self, _req: &Request<'_>, ino: u64, _fh: u64, offset: i64, data: &[u8], _write_flags: u32, _flags: i32, _lock_owner: Option<u64>, reply: fuser::ReplyWrite) {

        let node = match self.nodes.get(&ino) {
            Some(x) => x,
            None => return reply.error(ENOENT)
        };

        let file = match &node.kind {
            FileType::File(f) => f,
            FileType::Dir(_) => return reply.error(EISDIR),
        };

        let tmp_path = match &file.tmp_file {
            Some(p) => p.clone(),
            None => return reply.error(EIO),
        };

        dbg!(&tmp_path);

        let mut f = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&tmp_path)
        {
            Ok(f) => f,
            Err(e) => {
                dbg!(e);
                return reply.error(EIO);
            }
        };

        if offset < 0 {
            return reply.error(EIO);
        }

        if f.seek(SeekFrom::Start(offset as u64)).is_err() {
            return reply.error(EIO);
        }

        if f.write_all(data).is_err() {
            return reply.error(EIO);
        }

        self.offset = Some(offset as u64);

        reply.written(data.len() as u32);
    }

    fn create(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, _mode: u32, _umask: u32, _flags: i32, reply: fuser::ReplyCreate) {
        let filename = name.to_str().unwrap().to_string();

        let ino = self.alloc_ino();

        let file = File {
            name: filename.clone(),
            api: String::new(),
            size: 0,
            ino: ino,
            tmp_file: Some(format!("/tmp/FS/{}", filename)),
            cnk_id: 0,
            sync: false
        };

        let node = Node {
            ino,
            name: file.name.clone(),
            kind: FileType::File(file),
            parent
        };

        self.nodes.insert(ino, node.clone());

        reply.created(&TTL, &node_to_attr(&node), 0, 0, 0);
    }

    fn flush(&mut self, _req: &Request<'_>, ino: u64, fh: u64, lock_owner: u64, reply: fuser::ReplyEmpty) {

    }
}
