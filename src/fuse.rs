use crate::github::{ FileTree, FileType, Node };
use fuser::{
    ReplyAttr, FileAttr, Filesystem, ReplyDirectory, ReplyEntry, Request, FileType as FuserFileType
};
use libc::ENOENT;
use std::time::{Duration, SystemTime};
use tokio::runtime::Handle;
use std::ffi::OsStr;

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

        let data = self.handle.block_on(
            self.github.as_mut()
                .expect("[!] Error: not github set idk")
                .download_file(&file.file)
        );

        let offset = offset as usize;
        if offset >= data.len() {
            return reply.data(&[]);
        }

        let end = std::cmp::min(offset + size as usize, data.len());
        reply.data(&data[offset..end]);
    }
}
