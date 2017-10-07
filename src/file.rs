use std::cmp;
use std::io::prelude::*;
use std::io::{SeekFrom, ErrorKind};
use std::io;

use fs::FileSystemRef;

#[derive(Clone)]
pub struct File<'a, 'b: 'a> {
    first_cluster: Option<u32>,
    size: Option<u32>,
    offset: u32,
    current_cluster: Option<u32>,
    fs: FileSystemRef<'a, 'b>,
}

impl <'a, 'b> File<'a, 'b> {
    pub(crate) fn new(first_cluster: Option<u32>, size: Option<u32>, fs: FileSystemRef<'a, 'b>) -> Self {
        File {
            first_cluster, size, fs,
            current_cluster: first_cluster,
            offset: 0,
        }
    }
}

impl <'a, 'b> Read for File<'a, 'b> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut buf_offset: usize = 0;
        let cluster_size = self.fs.get_cluster_size();
        loop {
            let offset_in_cluster = self.offset % cluster_size;
            let bytes_left_in_cluster = (cluster_size - offset_in_cluster) as usize;
            let bytes_left_in_file = self.size.map(|size| (size - self.offset) as usize).unwrap_or(bytes_left_in_cluster);
            let bytes_left_in_buf = buf.len() - buf_offset;
            let read_size = cmp::min(cmp::min(bytes_left_in_buf, bytes_left_in_cluster), bytes_left_in_file);
            if read_size == 0 || self.current_cluster.is_none() {
                break;
            }
            let current_cluster = self.current_cluster.unwrap();
            let offset_in_fs = self.fs.offset_from_cluster(current_cluster) + (offset_in_cluster as u64);
            let read_bytes = {
                let mut rdr = self.fs.rdr.borrow_mut();
                rdr.seek(SeekFrom::Start(offset_in_fs))?;
                rdr.read(&mut buf[buf_offset..buf_offset+read_size])?
            };
            if read_bytes == 0 {
                break;
            }
            self.offset += read_bytes as u32;
            buf_offset += read_bytes;
            if self.offset % cluster_size == 0 {
                let r = self.fs.cluster_iter(current_cluster).skip(1).next();
                self.current_cluster = match r {
                    Some(Err(err)) => return Err(err),
                    Some(Ok(n)) => Some(n),
                    None => None,
                };
            }
        }
        Ok(buf_offset)
    }
}

impl <'a, 'b> Seek for File<'a, 'b> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            SeekFrom::Current(x) => self.offset as i64 + x,
            SeekFrom::Start(x) => x as i64,
            SeekFrom::End(x) => self.size.unwrap() as i64 + x,
        };
        if new_offset < 0 || (self.size.is_some() && new_offset as u64 > self.size.unwrap() as u64) {
            return Err(io::Error::new(ErrorKind::InvalidInput, "invalid seek"));
        }
        let cluster_size = self.fs.get_cluster_size();
        let cluster_count = (new_offset / cluster_size as i64) as usize;
        let new_cluster = if cluster_count > 0 {
            match self.fs.cluster_iter(self.first_cluster.unwrap()).skip(cluster_count).next() {
                Some(Err(err)) => return Err(err),
                Some(Ok(n)) => Some(n),
                None => None,
            }
        } else {
            self.first_cluster
        };
        self.offset = new_offset as u32;
        self.current_cluster = new_cluster;
        Ok(self.offset as u64)
    }
}
