use std::collections::HashMap;

use async_trait::async_trait;
use nfsserve::{
    nfs::{fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, specdata3},
    tcp::{NFSTcp, NFSTcpListener},
    vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities},
};

const ROOT_ID: fileid3 = 1;
const INDEX_ID: fileid3 = 2;
const INDEX_FILE_NAME: &str = "index.json";
const METADATA_FILE_NAME: &str = "metadata.json";

pub struct NfsDataset {
    pub name: String,
    pub kind: String,
    pub entries: Vec<NfsEntry>,
}

pub struct NfsEntry {
    pub id: String,
    pub metadata_json: String,
}

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

pub struct CybershuttleNfs {
    nodes: HashMap<fileid3, NfsNode>,
}

impl CybershuttleNfs {
    pub fn new(datasets: Vec<NfsDataset>) -> Self {
        let mut nodes = HashMap::new();
        let index_json = nfs_index_json(&datasets);
        let mut root_children = vec![INDEX_ID];
        let mut next_id: fileid3 = 3;

        for dataset in datasets {
            let dataset_dir_id = next_id;
            next_id += 1;
            root_children.push(dataset_dir_id);

            let mut dataset_children = Vec::new();

            for entry in dataset.entries {
                let entry_dir_id = next_id;
                next_id += 1;

                let metadata_file_id = next_id;
                next_id += 1;

                dataset_children.push(entry_dir_id);

                nodes.insert(
                    entry_dir_id,
                    NfsNode::Directory {
                        name: entry.id.as_bytes().to_vec(),
                        parent: dataset_dir_id,
                        children: vec![metadata_file_id],
                    },
                );

                nodes.insert(
                    metadata_file_id,
                    NfsNode::File {
                        name: METADATA_FILE_NAME.as_bytes().to_vec(),
                        contents: entry.metadata_json.as_bytes().to_vec(),
                    },
                );
            }

            nodes.insert(
                dataset_dir_id,
                NfsNode::Directory {
                    name: dataset.name.as_bytes().to_vec(),
                    parent: ROOT_ID,
                    children: dataset_children,
                },
            );
        }

        nodes.insert(
            INDEX_ID,
            NfsNode::File {
                name: INDEX_FILE_NAME.as_bytes().to_vec(),
                contents: index_json.into_bytes(),
            },
        );

        nodes.insert(
            ROOT_ID,
            NfsNode::Directory {
                name: b"/".to_vec(),
                parent: ROOT_ID,
                children: root_children,
            },
        );

        CybershuttleNfs { nodes }
    }
}

fn nfs_index_json(datasets: &[NfsDataset]) -> String {
    let dataset_values: Vec<serde_json::Value> = datasets
        .iter()
        .map(|dataset| {
            serde_json::json!({
                "name": dataset.name.as_str(),
                "kind": dataset.kind.as_str(),
                "entries": dataset.entries.len(),
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({ "datasets": dataset_values })).unwrap()
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
impl NFSFileSystem for CybershuttleNfs {
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

pub async fn serve(datasets: Vec<NfsDataset>, bind_addr: &str) -> std::io::Result<()> {
    let fs = CybershuttleNfs::new(datasets);

    let listener = NFSTcpListener::bind(bind_addr, fs).await?;
    println!("Serving Cybershuttle NFS on {bind_addr}");

    listener.handle_forever().await
}
