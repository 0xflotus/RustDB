use crate::cache::Cache;
use crate::*;

struct SPSInner {
    file: CompactFile,
    stash: Cache<Data>,
    cache: HashMap<u64, Data>,
}

/// Allows logical database pages to be shared to allow concurrent readers.
pub struct SharedPagedData {
    x: Mutex<SPSInner>,
}

impl SharedPagedData {
    /// Construct new SharedPageData based on specified underlying storage.
    pub fn new(file: Box<dyn Storage + Send>) -> Self {
        Self {
            x: Mutex::new(SPSInner {
                file: CompactFile::new(file, 400, 1024),
                stash: Cache::new(),
                cache: HashMap::new(),
            }),
        }
    }
    /// Access to a virtual read-only copy of the database logical pages.
    pub fn open_read(self: &Arc<SharedPagedData>) -> AccessPagedData {
        let mut x = self.x.lock().unwrap();
        AccessPagedData {
            writer: false,
            time: x.stash.begin_read(),
            spd: self.clone(),
        }
    }

    /// Write access to the database logical pages.
    pub fn open_write(self: &Arc<SharedPagedData>) -> AccessPagedData {
        AccessPagedData {
            writer: true,
            time: 0,
            spd: self.clone(),
        }
    }

    fn end_read(&self, time: u64) {
        let mut x = self.x.lock().unwrap();
        x.stash.end_read(time);
    }

    fn set_page(&self, lpnum: u64, p: Data) {
        let mut x = self.x.lock().unwrap();
        x.file.set_page(lpnum, &p, p.len());
        let old = {
            if let Some(old) = x.cache.get(&lpnum) {
                old.clone()
            } else {
                Arc::new(Vec::new())
            }
        };
        x.stash.set(lpnum, old);
        x.cache.insert(lpnum, p);
    }

    fn get_page(&self, lpnum: u64, time: u64) -> Data {
        // println!("get_page lpnum={} time={}", lpnum, time );
        let mut x = self.x.lock().unwrap();
        if let Some(p) = x.stash.get(lpnum, time) {
            // println!("got page from stash, lpnum={}", lpnum );
            p.clone()
        } else if let Some(p) = x.cache.get(&lpnum) {
            // println!("got page from cache, lpnum={}", lpnum );
            p.clone()
        } else {
            // println!("got page from file, lpnum={}", lpnum );
            let n = x.file.page_size(lpnum);
            let mut v = vec![0; n];
            x.file.get_page(lpnum, &mut v);
            let p = Arc::new(v);
            x.cache.insert(lpnum, p.clone());
            p
        }
    }

    fn direct_get_page(&self, lpnum: u64) -> Data {
        let mut x = self.x.lock().unwrap();
        if let Some(p) = x.cache.get(&lpnum) {
            p.clone()
        } else {
            let n = x.file.page_size(lpnum);
            let mut v = vec![0; n];
            x.file.get_page(lpnum, &mut v);
            Arc::new(v)
        }
    }
}

/// Access to paged data.
pub struct AccessPagedData {
    writer: bool,
    time: u64,
    spd: Arc<SharedPagedData>,
}

impl AccessPagedData {
    /// Get the specified page.
    pub fn get_page(&self, lpnum: u64) -> Data {
        if self.writer {
            self.spd.direct_get_page(lpnum)
        } else {
            self.spd.get_page(lpnum, self.time)
        }
    }
    /// Is the underlying file new (so needs to be initialised ).
    pub fn is_new(&self) -> bool {
        self.writer && self.spd.x.lock().unwrap().file.is_new()
    }
    /// Check whether to compress a page.
    pub fn compress(&self, size: usize, saving: usize) -> bool {
        debug_assert!(self.writer);
        self.spd.x.lock().unwrap().file.compress(size, saving)
    }
    /// Set the data of the specified page.
    pub fn set_page(&self, lpnum: u64, p: Data) {
        debug_assert!(self.writer);
        self.spd.set_page(lpnum, p);
    }
    /// Allocate a logical page.
    pub fn alloc_page(&self) -> u64 {
        debug_assert!(self.writer);
        self.spd.x.lock().unwrap().file.alloc_page()
    }
    /// Free a logical page.
    pub fn free_page(&self, lpnum: u64) {
        debug_assert!(self.writer);
        self.spd.x.lock().unwrap().file.free_page(lpnum)
    }
    /// Commit changes to underlying file ( or rollback logical page allocations ).
    pub fn save(&self, op: SaveOp) {
        debug_assert!(self.writer);
        let mut x = self.spd.x.lock().unwrap();
        match op {
            SaveOp::Save => {
                x.file.save();
                x.stash.tick();
            }
            SaveOp::RollBack => {
                x.file.rollback();
            }
        }
    }
}

impl Drop for AccessPagedData {
    fn drop(&mut self) {
        if !self.writer {
            self.spd.end_read(self.time);
        }
    }
}
