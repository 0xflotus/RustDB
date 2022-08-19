use crate::{
    nd, Arc, BTreeMap, CompactFile, Data, HashMap, HashSet, Mutex, RwLock, SaveOp, Storage,
};
use std::ops::Bound::Included;

/// ```Arc<Mutex<PageInfo>>```
type PageInfoPtr = Arc<Mutex<PageInfo>>;

/// Cached information about a logical page.
struct PageInfo {
    /// Current data for the page.
    current: Option<Data>,
    /// Historic data for the page.
    history: BTreeMap<u64, Data>,
    /// Count of how manay times the page has been used.
    counter: usize,
    /// Position of the page in stash heap.
    heap_pos: usize,
}

impl PageInfo {
    /// Construct a new PageInfo.
    fn new() -> PageInfoPtr {
        Arc::new(Mutex::new(Self {
            current: None,
            history: BTreeMap::new(),
            heap_pos: usize::MAX,
            counter: 0,
        }))
    }

    /// Get the Data for the page, checking history if not a writer.
    /// Reads Data from file if necessary.
    /// Result is Data and flag indicating that data was read from file.
    fn get(&mut self, lpnum: u64, a: &AccessPagedData) -> (Data, bool) {
        if !a.writer {
            if let Some((_k, v)) = self
                .history
                .range((Included(&a.time), Included(&u64::MAX)))
                .next()
            {
                return (v.clone(), false);
            }
        }

        if let Some(p) = &self.current {
            return (p.clone(), false);
        }

        // Get data from file.
        let file = a.spd.file.read().unwrap();
        let data = file.get_page(lpnum);
        self.current = Some(data.clone());
        (data, true)
    }

    /// Set the page data, updating the history using the specified time and current data.
    /// result is size of old data (if any).
    fn set(&mut self, time: u64, data: Data) -> usize {
        let mut result = 0;
        if let Some(old) = self.current.take() {
            result = old.len();
            self.history.insert(time, old);
        }
        self.current = Some(data);
        result
    }

    /// Reduce the history to the specified cache time.
    fn trim(&mut self, to: u64) {
        while let Some(&f) = self.history.keys().next() {
            if f >= to {
                break;
            }
            self.history.remove(&f);
        }
    }
}

/// Heap keeps track of the page with the smallest counter.
#[derive(Default)]
struct Heap {
    v: Vec<PageInfoPtr>,
}

impl Heap {
    /// Increases counter for p and adjusts the heap to match.
    fn used(&mut self, p: PageInfoPtr) -> PageInfoPtr {
        let (mut pos, counter) = {
            let mut p = p.lock().unwrap();
            p.counter += 1;
            (p.heap_pos, p.counter)
        };
        if pos == usize::MAX {
            pos = self.v.len();
            self.v.push(p.clone());
            self.move_up(pos, counter);
        } else {
            self.move_down(pos, counter);
        }
        p
    }

    fn pop(&mut self) -> usize {
        let mut result = 0;
        {
            let mut p = self.v[0].lock().unwrap();
            if let Some(d) = &p.current {
                result = d.len();
                p.current = None;
                p.heap_pos = usize::MAX;
            }
        }
        // Pop the last element of the vector, save in position zero.
        let last = self.v.pop().unwrap();
        let counter = last.lock().unwrap().counter;
        self.v[0] = last;
        // Restore heap invariant.
        self.move_down(0, counter);
        result
    }

    /// Called when page at pos may be too low in the heap.
    fn move_up(&mut self, mut pos: usize, counter: usize) {
        loop {
            if pos == 0 {
                break;
            }
            let ppos = (pos - 1) / 2;
            {
                let mut pl = self.v[ppos].lock().unwrap();
                if pl.counter <= counter {
                    break;
                }
                pl.heap_pos = pos;
            }
            self.v.swap(ppos, pos);
            pos = ppos;
        }
        self.v[pos].lock().unwrap().heap_pos = pos;
    }

    /// Called when page at pos may be too high in the heap.
    fn move_down(&mut self, mut pos: usize, counter: usize) {
        let n = self.v.len();
        loop {
            let mut cpos = pos * 2 + 1;
            if cpos >= n {
                break;
            } else {
                let mut c1 = self.v[cpos].lock().unwrap().counter;
                if cpos + 1 < n {
                    let c2 = self.v[cpos + 1].lock().unwrap().counter;
                    if c2 < c1 {
                        cpos += 1;
                        c1 = c2;
                    }
                }
                if counter <= c1 {
                    break;
                }
            }
            self.v.swap(pos, cpos);
            self.v[pos].lock().unwrap().heap_pos = pos;
            pos = cpos;
        }
        self.v[pos].lock().unwrap().heap_pos = pos;
    }

    /// For debugging.
    fn _check(&self) -> usize {
        let mut total = 0;
        for x in 0..self.v.len() {
            let p = &*self.v[x].lock().unwrap();
            if let Some(d) = &p.current {
                total += d.len();
            }
            debug_assert!(x == p.heap_pos);
            if x * 2 + 1 < self.v.len() {
                let cc = self.v[x * 2 + 1].lock().unwrap().counter;
                if cc < p.counter {
                    println!("cc1 check failed x={} cc={} p.counter={}", x, cc, p.counter);
                    loop {}
                }
            }
            if x * 2 + 2 < self.v.len() {
                let cc = self.v[x * 2 + 2].lock().unwrap().counter;
                if cc < p.counter {
                    println!("cc2 check failed x={} cc={} p.counter={}", x, cc, p.counter);
                    loop {}
                }
            }
        }
        total
    }
}

/// Central store of data.
#[derive(Default)]
pub struct Stash {
    /// Write time - number of writes.
    time: u64,
    /// Page number -> page info.
    pages: HashMap<u64, PageInfoPtr>,
    /// Time -> reader count.
    readers: BTreeMap<u64, usize>,
    /// Time -> set of page numbers.
    updates: BTreeMap<u64, HashSet<u64>>,
    /// Total size of current pages.
    pub total: usize,
    /// trim_cache reduces total to mem_limit (or below).
    pub mem_limit: usize,
    /// Heap of pages, page with smallest counter in position 0.
    heap: Heap,
    /// Trace cache trimming etc.
    pub trace: bool,
}

impl Stash {
    /// Adjust page info to reflect page has been used.
    fn used(&mut self, p: PageInfoPtr) -> PageInfoPtr {
        let p = self.heap.used(p);
        debug_assert!(self.heap._check() == self.total);
        p
    }

    /// Set the value of the specified page for the current time.
    fn set(&mut self, lpnum: u64, data: Data) {
        let time = self.time;
        let u = self.updates.entry(time).or_insert_with(HashSet::default);
        if u.insert(lpnum) {
            let mut p = self
                .pages
                .entry(lpnum)
                .or_insert_with(PageInfo::new)
                .clone();
            p = self.used(p);
            self.total += data.len();
            self.total -= p.lock().unwrap().set(time, data);
        }
    }

    /// Get the PageInfoPtr for the specified page and insert into lru chain.
    fn get(&mut self, lpnum: u64) -> PageInfoPtr {
        let p = self.pages.entry(lpnum).or_insert_with(PageInfo::new).clone();
        self.used(p)
    }

    /// Register that there is a client reading the database. The result is the current time.
    fn begin_read(&mut self) -> u64 {
        let time = self.time;
        let n = self.readers.entry(time).or_insert(0);
        *n += 1;
        time
    }

    /// Register that the read at the specified time has ended. Stashed pages may be freed.
    fn end_read(&mut self, time: u64) {
        let n = self.readers.get_mut(&time).unwrap();
        *n -= 1;
        if *n == 0 {
            self.readers.remove(&time);
            self.trim();
        }
    }

    /// Register that an update operation has completed. Time is incremented.
    /// Stashed pages may be freed.
    fn end_write(&mut self) -> usize {
        let result = if let Some(u) = self.updates.get(&self.time) {
            u.len()
        } else {
            0
        };
        self.time += 1;
        self.trim();
        result
    }

    /// Trim due to a read or write ending.
    fn trim(&mut self) {
        // rt is time of first remaining reader.
        let rt = *self.readers.keys().next().unwrap_or(&self.time);
        // wt is time of first remaining update.
        while let Some(&wt) = self.updates.keys().next() {
            if wt >= rt {
                break;
            }
            for lpnum in self.updates.remove(&wt).unwrap() {
                let p = self.pages.get(&lpnum).unwrap();
                p.lock().unwrap().trim(rt);
            }
        }
    }

    /// Trim cached data ( to reduce memory usage ).
    fn trim_cache(&mut self) {
        let (old_total, old_len) = (self.total, self.heap.v.len());
        while !self.heap.v.is_empty() && self.total >= self.mem_limit {
            self.total -= self.heap.pop();
        }
        if self.trace {
                debug_assert!(self.heap._check() == self.total);
                let (new_total, new_len) = (self.total, self.heap.v.len());
                println!(
                    "trimmed cache mem_limit={} total={}(-{}) heap len={}(-{})",
                    self.mem_limit,
                    new_total,
                    old_total - new_total,
                    new_len,
                    old_len - new_len
                );
        }
    }
}

/// Allows logical database pages to be shared to allow concurrent readers.
pub struct SharedPagedData {
    ///
    pub file: RwLock<CompactFile>,
    ///
    pub sp_size: usize,
    ///
    pub ep_size: usize,
    ///
    pub stash: RwLock<Stash>,
}

/// =1024. Size of an extension page.
const EP_SIZE: usize = 1024;
/// =16. Maximum number of extension pages.
const EP_MAX: usize = 16;
/// =136. Starter page size.
const SP_SIZE: usize = (EP_MAX + 1) * 8;

impl SharedPagedData {
    /// Construct SharedPageData based on specified underlying storage.
    pub fn new(file: Box<dyn Storage>) -> Self {
        let file = CompactFile::new(file, SP_SIZE, EP_SIZE);
        // Note : if it's not a new file, sp_size and ep_size are read from file header.
        let sp_size = file.sp_size;
        let ep_size = file.ep_size;
        Self {
            stash: RwLock::new(Stash::default()),
            file: RwLock::new(file),
            sp_size,
            ep_size,
        }
    }

    /// Calculate the maxiumum size of a logical page. This value is stored in the Database struct.
    pub fn page_size_max(&self) -> usize {
        let ep_max = (self.sp_size - 2) / 8;
        (self.ep_size - 16) * ep_max + (self.sp_size - 2)
    }

    /// Trim cache.
    pub fn trim_cache(&self) {
        self.stash.write().unwrap().trim_cache();
    }
}

/// Access to shared paged data.
pub struct AccessPagedData {
    writer: bool,
    time: u64,
    ///
    pub spd: Arc<SharedPagedData>,
}

impl AccessPagedData {
    /// Construct access to a virtual read-only copy of the database logical pages.
    pub fn new_reader(spd: Arc<SharedPagedData>) -> Self {
        let time = spd.stash.write().unwrap().begin_read();
        AccessPagedData {
            writer: false,
            time,
            spd,
        }
    }

    /// Construct access to the database logical pages.
    pub fn new_writer(spd: Arc<SharedPagedData>) -> Self {
        AccessPagedData {
            writer: true,
            time: 0,
            spd,
        }
    }

    /// Get the Data for the specified page.
    pub fn get_page(&self, lpnum: u64) -> Data {
        let mut stash = self.spd.stash.write().unwrap();

        // Get PageInfoPtr for the specified page.
        let pinfo = stash.get(lpnum);

        // Lock the Mutex for the page.
        let mut pinfo = pinfo.lock().unwrap();

        // Read the page data.
        let (data, loaded) = pinfo.get(lpnum, self);
        if loaded {
            stash.total += data.len();
        }
        data
    }

    /// Set the data of the specified page.
    pub fn set_page(&self, lpnum: u64, data: Data) {
        debug_assert!(self.writer);

        // First update the stash ( ensures any readers will not attempt to read the file ).
        self.spd.stash.write().unwrap().set(lpnum, data.clone());

        // Write data to underlying file.
        self.spd.file.write().unwrap().set_page(lpnum, data);
    }

    /// Is the underlying file new (so needs to be initialised ).
    pub fn is_new(&self) -> bool {
        self.writer && self.spd.file.read().unwrap().is_new()
    }

    /// Check whether compressing a page is worthwhile.
    pub fn compress(&self, size: usize, saving: usize) -> bool {
        debug_assert!(self.writer);
        CompactFile::compress(self.spd.sp_size, self.spd.ep_size, size, saving)
    }

    /// Allocate a logical page.
    pub fn alloc_page(&self) -> u64 {
        debug_assert!(self.writer);
        self.spd.file.write().unwrap().alloc_page()
    }

    /// Free a logical page.
    pub fn free_page(&self, lpnum: u64) {
        debug_assert!(self.writer);
        self.spd.stash.write().unwrap().set(lpnum, nd());
        self.spd.file.write().unwrap().free_page(lpnum);
    }

    /// Commit changes to underlying file ( or rollback logical page allocations ).
    pub fn save(&self, op: SaveOp) -> usize {
        debug_assert!(self.writer);
        match op {
            SaveOp::Save => {
                self.spd.file.write().unwrap().save();
                self.spd.stash.write().unwrap().end_write()
            }
            SaveOp::RollBack => {
                // Note: rollback happens before any pages are updated.
                // However logical page allocations need to be rolled back.
                self.spd.file.write().unwrap().rollback();
                0
            }
        }
    }
}

impl Drop for AccessPagedData {
    fn drop(&mut self) {
        if !self.writer {
            self.spd.stash.write().unwrap().end_read(self.time);
        }
    }
}
