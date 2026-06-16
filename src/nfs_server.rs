use std::collections::HashMap;

use async_trait::async_trait;
use nfsserve::{
    nfs::{fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, specdata3},
    tcp::{NFSTcp, NFSTcpListener},
    vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities},
};

use crate::atlas::AtlasDataSource;

const ROOT_ID: fileid3 = 1;
const ATLAS_ID: fileid3 = 2;
const METADATA_FILE_NAME: &str = "metadata.json";

enum NfsNode {
    Directory {
        name: Vec<u8>,
        parent: fileid3,
        children: Vec<fileid3>,
    },
    File {
        name: Vec<u8>,
        contents: Vec<u8>,
    },
}

pub struct AtlasNfs {
    nodes: HashMap<fileid3, NfsNode>,
}

impl AtlasNfs {
    pub fn new(atlas: AtlasDataSource) -> Self {
        let mut nodes = HashMap::new();

        nodes.insert(
            ROOT_ID,
            NfsNode::Directory {
                name: b"/".to_vec(),
                parent: ROOT_ID,
                children: vec![ATLAS_ID],
            },
        );

        let mut atlas_children = Vec::new();
        let mut next_id: fileid3 = 3;

        for entry_id in atlas.entry_ids() {
            let entry_dir_id = next_id;
            next_id += 1;

            let metadata_file_id = next_id;
            next_id += 1;

            atlas_children.push(entry_dir_id);

            nodes.insert(
                entry_dir_id,
                NfsNode::Directory {
                    name: entry_id.as_bytes().to_vec(),
                    parent: ATLAS_ID,
                    children: vec![metadata_file_id],
                },
            );

            let metadata = atlas
                .metadata_json(&entry_id)
                .unwrap_or("")
                .as_bytes()
                .to_vec();

            nodes.insert(
                metadata_file_id,
                NfsNode::File {
                    name: METADATA_FILE_NAME.as_bytes().to_vec(),
                    contents: metadata,
                },
            );
        }

        nodes.insert(
            ATLAS_ID,
            NfsNode::Directory {
                name: b"atlas".to_vec(),
                parent: ROOT_ID,
                children: atlas_children,
            },
        );

        AtlasNfs { nodes }
    }
}

fn attr_for(id: fileid3, node: &NfsNode) -> fattr3 {
    let (ftype, size, mode) = match node {
        NfsNode::Directory { .. } => (ftype3::NF3DIR, 0, 0o755),
        NfsNode::File { contents, .. } => (ftype3::NF3REG, contents.len() as u64, 0o644),
    };

    fattr3 {
        ftype,
        mode,
        nlink: 1,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        size,
        used: size,
        rdev: specdata3::default(),
        fsid: 0,
        fileid: id,
        atime: nfstime3::default(),
        mtime: nfstime3::default(),
        ctime: nfstime3::default(),
    }
}

#[async_trait]
impl NFSFileSystem for AtlasNfs {
    fn root_dir(&self) -> fileid3 {
        ROOT_ID
    }

    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadOnly
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let Some(NfsNode::Directory {
            parent, children, ..
        }) = self.nodes.get(&dirid)
        else {
            return Err(nfsstat3::NFS3ERR_NOTDIR);
        };

        if filename.as_slice() == b"." {
            return Ok(dirid);
        }

        if filename.as_slice() == b".." {
            return Ok(*parent);
        }

        for child_id in children {
            let child = self.nodes.get(child_id).ok_or(nfsstat3::NFS3ERR_NOENT)?;
            let child_name = match child {
                NfsNode::Directory { name, .. } => name,
                NfsNode::File { name, .. } => name,
            };

            if child_name.as_slice() == filename.as_slice() {
                return Ok(*child_id);
            }
        }

        Err(nfsstat3::NFS3ERR_NOENT)
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        let node = self.nodes.get(&id).ok_or(nfsstat3::NFS3ERR_NOENT)?;
        Ok(attr_for(id, node))
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let node = self.nodes.get(&id).ok_or(nfsstat3::NFS3ERR_NOENT)?;

        let NfsNode::File { contents, .. } = node else {
            return Err(nfsstat3::NFS3ERR_ISDIR);
        };

        let start = (offset as usize).min(contents.len());
        let end = (start + count as usize).min(contents.len());
        let eof = end >= contents.len();

        Ok((contents[start..end].to_vec(), eof))
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let Some(NfsNode::Directory { children, .. }) = self.nodes.get(&dirid) else {
            return Err(nfsstat3::NFS3ERR_NOTDIR);
        };

        let start_index = if start_after == 0 {
            0
        } else {
            children
                .iter()
                .position(|id| *id == start_after)
                .map(|pos| pos + 1)
                .ok_or(nfsstat3::NFS3ERR_BAD_COOKIE)?
        };

        let mut entries = Vec::new();

        for child_id in children.iter().skip(start_index).take(max_entries) {
            let node = self.nodes.get(child_id).ok_or(nfsstat3::NFS3ERR_NOENT)?;
            let name = match node {
                NfsNode::Directory { name, .. } => name.clone(),
                NfsNode::File { name, .. } => name.clone(),
            };

            entries.push(DirEntry {
                fileid: *child_id,
                name: name.into(),
                attr: attr_for(*child_id, node),
            });
        }

        let end = start_index + entries.len() >= children.len();

        Ok(ReadDirResult { entries, end })
    }

    async fn write(&self, _id: fileid3, _offset: u64, _data: &[u8]) -> Result<fattr3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn create(
        &self,
        _dirid: fileid3,
        _filename: &filename3,
        _attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn create_exclusive(
        &self,
        _dirid: fileid3,
        _filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn mkdir(
        &self,
        _dirid: fileid3,
        _dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn remove(&self, _dirid: fileid3, _filename: &filename3) -> Result<(), nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn rename(
        &self,
        _from_dirid: fileid3,
        _from_filename: &filename3,
        _to_dirid: fileid3,
        _to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn setattr(&self, _id: fileid3, _setattr: sattr3) -> Result<fattr3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn symlink(
        &self,
        _dirid: fileid3,
        _linkname: &filename3,
        _symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        Err(nfsstat3::NFS3ERR_ROFS)
    }

    async fn readlink(&self, _id: fileid3) -> Result<nfspath3, nfsstat3> {
        Err(nfsstat3::NFS3ERR_NOTSUPP)
    }
}

pub async fn serve(atlas: AtlasDataSource, bind_addr: &str) -> std::io::Result<()> {
    let fs = AtlasNfs::new(atlas);

    let listener = NFSTcpListener::bind(bind_addr, fs).await?;
    println!("Serving ATLAS NFS on {bind_addr}");

    listener.handle_forever().await
}
