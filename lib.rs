//!
//!ToDo List:
//!
//!Optimise WHERE condition for UPDATE and DELETE.
//!
//!Decimal shifting when scales do not match.
//!
//!Multi-column index use from WHERE.
//!
//!multipart requests ( for file upload ).
//!
//!Implement DROP TABLE, DROP INDEX, DROP FUNCTION etc.
//!
//!Implement ALTER TABLE.
//!
//!Fully implement CREATE INDEX.
//!
//!Handle HTTP IO in parallel. Read-only transactions.
//!
//! Database with SQL-like language.
//! Example program:
//! ```
//! use std::net::TcpListener;
//! use database::{Database,spf::SimplePagedFile,web::WebQuery};
//! fn main()
//! {
//!     let file = Box::new( SimplePagedFile::new( "c:\\Users\\pc\\rust\\test01.rustdb" ) );
//!     let db = Database::new( file, INITSQL );    
//!     let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
//!     for tcps in listener.incoming()
//!     {
//!        let mut tcps = tcps.unwrap();
//!        let mut wq = WebQuery::new( &tcps ); // Reads the http request from the TCP stream into wq.
//!        db.run( SQL, &mut wq ); // Executes SQL, output is accumulated in wq.
//!        wq.write( &mut tcps ); // Writes the http response to the TCP stream.
//!        db.save(); // Saves database changes to disk.
//!     }
//! }
//! const SQL : &str = "SELECT 'hello world'";
//! const INITSQL : &str = "";
//!```
//!
//!General Design of Database
//!
//!Lowest level is SortedFile which stores fixed size Records.
//!
//!SortedFile is used to implement:
//!
//!(1) Variable length values ( which are split into fragments - see bytes module ).
//!
//!(2) Database Table storage. Each record has a 64-bit Id.
//!
//!(3) Index storage ( an index record refers back to the main table ).

/* Idea for compressing database file

(1) Scan Table and Index tables for roots.
(2) Scan parent pages for used pages.
(3) Now have bitmap with free pages, and also map ( pp -> page,offset for high-numbered pages pp ).
(4) Use free pages to relocate high-numbered pages (updating Table/Index or parent page).
(5) Truncate file.

Another idea:
At beginning of each page, store associated Index or Table Id.
Now to relocate a page, we find it's parent by starting from the root ( using a key from the page ).

Keep a list of free pages. When not busy, and have a free page, relocate last page in file to free page.

*/

use crate::{
  bytes::*, compile::*, eval::*, expr::*, page::*, parse::*, run::*, sortedfile::*, table::*, util::newmap, value::*,
};
use std::{cell::Cell, cell::RefCell, cmp::Ordering, collections::HashMap, panic, rc::Rc};

/// Utility functions and macros.
#[macro_use]
mod util;

/// WebQuery struct for making a http web server.
pub mod web;

/// Expression (uncompiled) types.
pub mod expr;

/// Compile parsed expressions, checking types.
pub mod compile;

/// Simple Paged File.
pub mod spf;

/// Value.
pub mod value;

/// Page for SortedFile.
///
/// A page is 0x4000 (16kb) bytes, logically divided into up to 2047 fixed size nodes, which implement a balanced binary tree.
///
/// Nodes are numbered from 1..2047, with 0 indicating a null ( non-existent ) node.
///
/// Each record has a 3 byte overhead, 2 bits to store the balance, 2 x 11 bits to store left and right node ids.
pub mod page;

// Private modules ( in principle, currently public ).

pub mod managedfile;

/// Table, ColInfo, Row and other Table types.
pub mod table;

/// Storage of variable length values : ByteStorage.
mod bytes;

/// Parser.
mod parse;

/// Access to system tables (Schema,Table,Column,Index,IndexColumn,Function).
mod sys;

/// Low-level sorted Record storage : SortedFile.
pub mod sortedfile;

/// Execution : Instruction (Inst) and other run time types.
mod run;

/// Execution : EvalEnv struct.
mod eval;

/// CExp implementations for basic expressions.
mod cexp;

/// Compilation of builtin functions.
mod builtin;

// End of modules.

/// ```Rc<Database>```
pub type DB = Rc<Database>;

/// Database with SQL-like interface.
pub struct Database
{
  /// Page storage.
  file: RefCell<Box<dyn PagedFile>>,
  sys_schema: TablePtr,
  sys_table: TablePtr,
  sys_column: TablePtr,
  sys_index: TablePtr,
  sys_index_col: TablePtr,
  /// Database is newly created.
  bs: ByteStorage,
  tables: RefCell<HashMap<ObjRef, TablePtr>>,
  schemas: RefCell<HashMap<String, i64>>,
  functions: RefCell<HashMap<ObjRef, FunctionPtr>>,
  builtins: RefCell<HashMap<String, (DataKind, CompileFunc)>>,
  functions_dirty: Cell<bool>,
  /// Last id generated by INSERT.
  lastid: Cell<i64>,
}

impl Database
{
  /// Construct a new DB, based on the specified file.
  pub fn new(mut file: Box<dyn PagedFile>, initsql: &str) -> DB
  {
    let mut cq = ConsoleQuery {};
    let is_new = file.is_new();
    if is_new
    {
      file.alloc_page(); // Allocate page for byte storage.
    }

    let mut tb = TableBuilder::new();

    let sys_schema = tb.nt("sys", "Schema", &[("Name", STRING)]);

    let sys_table = tb.nt(
      "sys",
      "Table",
      &[
        ("Root", BIGINT),
        ("Schema", BIGINT),
        ("Name", STRING),
        ("IsView", TINYINT),
        ("Def", STRING),
        ("IdGen", BIGINT),
      ],
    );

    let sys_column = tb.nt(
      "sys",
      "Column",
      &[("Table", BIGINT), ("Name", STRING), ("Type", BIGINT)],
    );

    let sys_index = tb.nt("sys", "Index", &[("Root", BIGINT), ("Table", BIGINT), ("Name", STRING)]);

    let sys_index_col = tb.nt("sys", "IndexColumn", &[("Index", BIGINT), ("ColId", BIGINT)]);

    sys_table.add_index(6, vec![1, 2]);
    sys_column.add_index(7, vec![0]);
    sys_index.add_index(8, vec![1]);
    sys_index_col.add_index(9, vec![0]);

    let db = Rc::new(Database {
      file: RefCell::new(file),
      sys_schema,
      sys_table,
      sys_column,
      sys_index,
      sys_index_col,
      bs: ByteStorage::new(0),
      schemas: newmap(),
      functions: newmap(),
      tables: newmap(),
      builtins: newmap(),
      functions_dirty: Cell::new(false),
      lastid: Cell::new(0),
    });

    db.bs.init(&db);

    for t in &tb.list
    {
      if !is_new
      {
        t.id_gen.set(sys::get_id_gen(&db, t.id as u64));
      }
      db.publish_table(t.clone());
    }

    if is_new
    {
      println!("New database... initialising");

      // The creation order has to match the order above ( so root values are as predicted ).
      let sysinit = "
CREATE SCHEMA sys
GO
CREATE TABLE sys.Schema( Name string )
CREATE TABLE sys.Table( Root bigint, Schema bigint, Name string, IsView tinyint, Def string, IdGen bigint )
CREATE TABLE sys.Column( Table bigint, Name string, Type bigint )
CREATE TABLE sys.Index( Root bigint, Table bigint, Name string )
CREATE TABLE sys.IndexColumn( Index bigint, ColId bigint )
GO
CREATE INDEX BySchemaName ON sys.Table(Schema,Name)
GO
CREATE INDEX ByTable ON sys.Column(Table)
CREATE INDEX ByTable ON sys.Index(Table)
CREATE INDEX ByIndex ON sys.IndexColumn(Index)
GO
CREATE TABLE sys.Function( Schema bigint, Name string, Def string )
GO
CREATE INDEX BySchemaName ON sys.Function(Schema,Name)
GO
";
      db.run(sysinit, &mut cq);
      db.run(initsql, &mut cq);
      db.save();
    }
    builtin::register_builtins(&db);
    db
  }

  /// Register a builtin function.
  pub fn register(self: &DB, name: &str, typ: DataKind, cf: CompileFunc)
  {
    self.builtins.borrow_mut().insert(name.to_string(), (typ, cf));
  }

  /// Run a batch of SQL.
  pub fn run(self: &DB, source: &str, qy: &mut dyn Query)
  {
    if let Some(e) = self.go(source, qy)
    {
      let err = format!(
        "Error : {} in {} at line {} column {}.",
        e.msg, e.rname, e.line, e.column
      );
      println!("Run error {}", &err);
      qy.set_error(err);
    }
  }

  /// Run a batch of SQL, printing the execution time.
  pub fn run_timed(self: &DB, source: &str, qy: &mut dyn Query)
  {
    let start = std::time::Instant::now();
    self.run(source, qy);
    println!("db run time={} micro sec.", start.elapsed().as_micros());
  }

  /// Run a batch of SQL.
  fn go(self: &DB, source: &str, qy: &mut dyn Query) -> Option<SqlError>
  {
    let mut p = Parser::new(source, self);

    let result = std::panic::catch_unwind(panic::AssertUnwindSafe(|| {
      p.batch(qy);
    }));

    if let Err(x) = result
    {
      Some(
        if let Some(e) = x.downcast_ref::<SqlError>()
        {
          SqlError { msg: e.msg.clone(), line: e.line, column: e.column, rname: e.rname.clone() }
        }
        else if let Some(s) = x.downcast_ref::<&str>()
        {
          p.make_error((*s).to_string())
        }
        else if let Some(s) = x.downcast_ref::<String>()
        {
          p.make_error(s.to_string())
        }
        else
        {
          p.make_error("unrecognised/unexpected error".to_string())
        },
      )
    }
    else
    {
      None
    }
  }

  /// Save updated tables to file.
  pub fn save(self: &DB)
  {
    self.bs.save(self);

    let tm = &*self.tables.borrow();
    for t in tm.values()
    {
      if t.id_gen_dirty.get()
      {
        sys::save_id_gen(self, t.id as u64, t.id_gen.get());
        t.id_gen_dirty.set(false);
      }
    }

    for t in tm.values()
    {
      t.save(self);
    }

    if self.functions_dirty.get()
    {
      for function in self.functions.borrow().values()
      {
        function.ilist.borrow_mut().clear();
      }
      self.functions.borrow_mut().clear();
      self.functions_dirty.set(false);
    }
    self.file.borrow_mut().save();
  }

  /// Print the tables ( for debugging ).
  pub fn dump_tables(self: &DB)
  {
    println!("Byte Storage");
    self.bs.file.dump();

    for (n, t) in &*self.tables.borrow()
    {
      println!("Dump Table {} {} {:?}", &n.schema, &n.name, t.info.colnames);
      t._dump(self);
    }
  }

  /// Get the named table.
  fn get_table(self: &DB, name: &ObjRef) -> Option<TablePtr>
  {
    if let Some(t) = self.tables.borrow().get(name)
    {
      return Some(t.clone());
    }
    sys::get_table(self, name)
  }

  /// Get the named function.
  fn get_function(self: &DB, name: &ObjRef) -> Option<FunctionPtr>
  {
    if let Some(f) = self.functions.borrow().get(name)
    {
      return Some(f.clone());
    }
    sys::get_function(self, name)
  }

  /// Insert the table into the map of tables.
  fn publish_table(&self, table: TablePtr)
  {
    let name = table.info.name.clone();
    self.tables.borrow_mut().insert(name, table);
  }

  /// Get code for value.
  fn encode(self: &DB, val: &Value) -> u64
  {
    let bytes = match val
    {
      Value::Binary(x) => x,
      Value::String(x) => x.as_bytes(),
      _ =>
      {
        return u64::MAX;
      }
    };
    if bytes.len() < 16
    {
      return u64::MAX;
    }
    self.bs.encode(self, &bytes[7..])
  }

  /// Decode u64 to bytes.
  fn decode(self: &DB, code: u64) -> Vec<u8>
  {
    self.bs.decode(self, code)
  }

  /// Delete encoding.
  fn delcode(self: &DB, code: u64)
  {
    self.bs.delcode(self, code);
  }

  /// Allocate a page of underlying file storage.
  fn alloc_page(self: &DB) -> u64
  {
    self.file.borrow_mut().alloc_page()
  }
} // end impl Database

impl Drop for Database
{
  /// Clear function instructions to avoid leaking memory.
  fn drop(&mut self)
  {
    for function in self.functions.borrow().values()
    {
      function.ilist.borrow_mut().clear();
    }
  }
}

/// For creating system tables.
struct TableBuilder
{
  alloc: i64,
  list: Vec<TablePtr>,
}

impl TableBuilder
{
  fn new() -> Self
  {
    Self { alloc: 1, list: Vec::new() }
  }

  fn nt(&mut self, schema: &str, name: &str, ct: &[(&str, DataType)]) -> TablePtr
  {
    let id = self.alloc;
    let root_page = id as u64;
    self.alloc += 1;
    let name = ObjRef::new(schema, name);
    let info = ColInfo::new(name, ct);
    let table = Table::new(id, root_page, 1, Rc::new(info));
    self.list.push(table.clone());
    table
  }
}

/// Backing storage for database tables.
pub trait PagedFile
{
  fn read_page(&mut self, pnum: u64, data: &mut [u8]);
  fn write_page(&mut self, pnum: u64, data: &[u8], size: usize);
  fn alloc_page(&mut self) -> u64;
  fn free_page(&mut self, pnum: u64);
  fn is_new(&self) -> bool;
  fn rollback(&mut self) {}
  fn save(&mut self) {}
}

/// IO Methods.
pub trait Query
{
  /// Append SELECT values to output.
  fn push(&mut self, values: &[Value]);

  /// ARG builtin function.
  fn arg(&mut self, _kind: i64, _name: &str) -> Rc<String>
  {
    Rc::new(String::new())
  }

  /// GLOBAL builtin function.
  fn global(&self, _kind: i64) -> i64
  {
    0
  }

  /// Set the error string.
  fn set_error(&mut self, err: String);

  /// Get the error string.
  fn get_error(&mut self) -> String
  {
    String::new()
  }
}

/// Query where output is printed to console.
pub struct ConsoleQuery {}

impl Query for ConsoleQuery
{
  fn push(&mut self, values: &[Value])
  {
    println!("{:?}", values);
  }

  /// Called when a panic ( error ) occurs.
  fn set_error(&mut self, err: String)
  {
    println!("Error: {}", err);
  }
}
