use crate::github::{ FileTree, FileType, Node};
use fuser::{
    FileAttr, Filesystem, ReplyEntry
};
use libc::ENOENT;
use std::time::{Duration, SystemTime};
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
            FileType::Dir(_) => 0o644,
            FileType::File(_) => 0o755
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
    fn lookup(&mut self, _req: &fuser::Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
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

    /*
    pub async fn readdir() {}
    pub async fn read(&mut self) {}
    pub async fn write() {}
    pub async fn create() {} */
}
