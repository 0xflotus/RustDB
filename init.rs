pub const INITSQL : &str = "
CREATE FUNCTION [sys].[TypeName]( t int ) RETURNS string AS 
BEGIN 
  RETURN CASE 
    WHEN t = 0 THEN 'none'
    WHEN t = 129 THEN 'binary'
    WHEN t = 130 THEN 'string' 
    WHEN t = 67 THEN 'bigint'
    WHEN t = 35 THEN 'int'
    WHEN t = 19 THEN 'smallint'
    WHEN t = 11 THEN 'tinyint'
    WHEN t = 68 THEN 'double'
    WHEN t = 46 THEN 'float'
    WHEN t = 13 THEN 'bool'
    WHEN t % 8 = 6 THEN 'decimal(' | ( t / 8 ) % 32 | ',' | t / 256 | ')'
    ELSE '??type??'
  END
END
GO
CREATE FUNCTION [sys].[TableName]( table int ) RETURNS string AS
BEGIN
  DECLARE schema int, name string
  SET schema = Schema, name = Name FROM sys.Table WHERE Id = table
  IF name = '' RETURN ''
  SET result = sys.Dot( Name, name ) FROM sys.Schema WHERE Id = schema
END
GO
CREATE FUNCTION [sys].[SingleQuote]( s string ) RETURNS string AS
BEGIN
  RETURN '''' | REPLACE( s, '''', '''''' ) | ''''
END
GO
CREATE FUNCTION [sys].[ScriptTable]( t int ) AS
BEGIN
  SELECT '
CREATE TABLE ' | sys.TableName(t) | sys.Cols(t) | ' 
GO'
  DECLARE ix int, name string
  FOR ix = Id, name = Name FROM sys.Index WHERE Table = t
  BEGIN
    SELECT '
CREATE INDEX ' | sys.QuoteName(name) | ' ON ' | sys.TableName(t) | sys.IndexCols(ix) | '
GO'
  END
END
GO
CREATE FUNCTION [sys].[ScriptSchemaBrowse]( s int ) AS
BEGIN
  DECLARE t int
  FOR t = Id FROM sys.Table WHERE Schema = s ORDER BY Name
  BEGIN
    EXEC sys.ScriptBrowse(t)
  END
END
GO
CREATE FUNCTION [sys].[ScriptSchema]( s int ) AS
BEGIN
  DECLARE sname string SET sname = sys.SchemaName(s)

  /* Create the schema, tables, indexes */
  
  IF sname != 'sys'
  BEGIN
    SELECT '
--############################################
CREATE SCHEMA ' | sys.QuoteName( sname )

    DECLARE t int
    FOR t = Id FROM sys.Table WHERE Schema = s ORDER BY Name
    BEGIN
      EXEC sys.ScriptTable(t)
    END
  END

  /******* Script functions *******/

  SELECT '
CREATE FUNCTION ' | sys.Dot( sname,Name) | Def | '
GO' 
  FROM sys.Function  WHERE Schema = s 

  /******* Script Data *******/

  IF sname != 'sys' AND sname != 'browse'
  BEGIN
    DECLARE ins string, val string
    FOR ins = '
INSERT INTO ' | sys.TableName(Id) | sys.ColNames(Id) | ' VALUES 
',
        val = 'SELECT ''(''|' | sys.ColValues(Id) | '|'')
''' | ' FROM ' | sys.TableName(Id)
    FROM sys.Table WHERE Schema = s ORDER BY Name
    BEGIN
      SELECT ins
      EXECUTE( val )
      SELECT 'GO
'
    END
  END
END
GO
CREATE FUNCTION [sys].[ScriptBrowse]( t int ) AS
BEGIN
  -- Script browse information for Table t.
  -- Looks up Table and Column Id values (tid,cid) by name in case they change.
  DECLARE sid int, tname string, sname string
  SET sid = Schema, tname = Name FROM sys.Table WHERE Id = t
  SET sname = Name FROM sys.Schema WHERE Id = sid

  SELECT '
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = ' | sys.SingleQuote(sname) | '
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = ' | sys.SingleQuote(tname) 
SELECT '
INSERT INTO browse.Table(Id,NameFunction, SelectFunction, DefaultOrder, Title, Description, Role) 
VALUES (tid,'
    | sys.SingleQuote(NameFunction) |','|sys.SingleQuote(SelectFunction) 
    | ',' | sys.SingleQuote(DefaultOrder) | ',' | sys.SingleQuote(Title) | ',' 
    | sys.SingleQuote(Description) | ',' | Role | ')'
  FROM browse.Table WHERE Id = t

  DECLARE cid int, cname string
  FOR cid=Id, cname=Name FROM sys.Column WHERE Table = t
  BEGIN
    SELECT '
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = ' | sys.SingleQuote(cname) | '
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, '
      |Position|','|sys.SingleQuote(Label)
      |','|sys.SingleQuote(Description)
      |','|RefersTo|','|sys.SingleQuote(Default)|','|InputCols|','|sys.SingleQuote(InputFunction)
      |','|InputRows|','|Style|','|sys.SingleQuote(DisplayFunction)|','|sys.SingleQuote(ParseFunction)|')'
    FROM browse.Column WHERE Id = cid
  END
  SELECT '
GO'
END
GO
CREATE FUNCTION [sys].[SchemaName]( schema int) RETURNS string AS 
BEGIN 
  SET result = Name FROM sys.Schema WHERE Id = schema
END
GO
CREATE FUNCTION [sys].[RecreateModifiedIndexes]() AS 
BEGIN
  DECLARE table int, name string, cols string
  FOR table = Table, name = Name, cols = sys.IndexCols( Id )
  FROM sys.Index WHERE Modified = 1
  BEGIN
    EXECUTE( 'DROP INDEX ' | name | ' ON ' | sys.TableName( table ) )
    EXECUTE( 'CREATE INDEX ' | name | ' ON ' | sys.TableName( table ) | cols )
  END
END
GO
CREATE FUNCTION [sys].[QuoteName]( s string ) RETURNS string AS
BEGIN
  RETURN '[' | REPLACE( s, ']', ']]' ) | ']'
END
GO
CREATE FUNCTION [sys].[ModifiedColumn]( t int, colId int ) AS 
BEGIN
  UPDATE sys.Index SET Modified = 1 WHERE Id IN ( SELECT Index FROM sys.IndexColumn WHERE Table = t AND ColId = colId )
END
GO
CREATE FUNCTION [sys].[IndexName]( index int ) RETURNS string AS
BEGIN
  SET result = sys.QuoteName(Name) FROM sys.Index WHERE Id = index
END
GO
CREATE FUNCTION [sys].[IndexCols]( index int ) RETURNS string AS
BEGIN
  DECLARE table int, list string, col string
  SET table = Table FROM sys.Index WHERE Id = index
  FOR col = sys.QuoteName(sys.ColName( table, ColId )) FROM sys.IndexColumn WHERE Index = index
    SET list |= CASE WHEN  list = '' THEN col ELSE ',' | col END
  RETURN '(' | list | ')'
END
GO
CREATE FUNCTION [sys].[DroppedColumn]( t int, colId int ) AS 
BEGIN 
  /* Called internally during ALTER TABLE */
  DECLARE index int
  WHILE 1 = 1 
  BEGIN
    SET index = 0
    SET index = Index FROM sys.IndexColumn WHERE Table = t AND ColId = colId
    IF index = 0 BREAK 
    EXECUTE( 'DROP INDEX ' | sys.IndexName(index) | ' ON ' | sys.TableName(t) )
  END
  UPDATE sys.IndexColumn SET ColId = ColId - 1 WHERE Table = t AND ColId >= colId
END
GO
CREATE FUNCTION [sys].[DropTable]( t int ) AS 
/* Note: this should not be called directly, instead use DROP TABLE statement */
BEGIN
  DECLARE id int
  /* Delete the Index data */
  FOR id = Id FROM sys.Index WHERE Table = t
  BEGIN
    DELETE FROM sys.IndexColumn WHERE Index = id
  END
  DELETE FROM sys.Index WHERE Table = t
   /* Delete the column data */
  FOR id = Id FROM sys.Column WHERE Table = t
  BEGIN
    DELETE FROM browse.Column WHERE Id = id
  END
  /* Delete other data */
  DELETE FROM browse.Table WHERE Id = t
  DELETE FROM sys.Column WHERE Table = t
  DELETE FROM sys.Table WHERE Id = t
END
GO
CREATE FUNCTION [sys].[DropSchema]( sid int ) AS
/* Note: this should not be called directly, instead use DROP SCHEMA statement */
BEGIN
  DECLARE schema string, name string
  SET schema = Name FROM sys.Schema WHERE Id = sid
  FOR name = Name FROM sys.Function WHERE Schema = sid EXECUTE( 'DROP FUNCTION ' | sys.Dot(schema,name) )
  -- FOR name = Name FROM sys.Table WHERE Schema = sid AND IsView = 1 EXECUTE( 'DROP VIEW ' | sys.Dot(schema,name) )
  FOR name = Name FROM sys.Table WHERE Schema = sid AND IsView = 0 EXECUTE( 'DROP TABLE ' | sys.Dot(schema,name) )
  DELETE FROM sys.Schema WHERE Id = sid
END
GO
CREATE FUNCTION [sys].[Dot]( schema string, name string ) RETURNS string AS
BEGIN
  RETURN sys.QuoteName( schema ) | '.' | sys.QuoteName( name )
END
GO
CREATE FUNCTION [sys].[Cols]( table int ) RETURNS string AS
BEGIN
  DECLARE col string, list string
  FOR col = sys.QuoteName(Name) | ' ' | sys.TypeName(Type)
  FROM sys.Column WHERE Table = table
    SET list |= CASE WHEN  list = '' THEN col ELSE ',' | col END
  RETURN '(' | list | ')'
END
GO
CREATE FUNCTION [sys].[ColValues]( table int ) RETURNS string AS
BEGIN
  DECLARE col string
  SET result = 'Id'
  FOR col = CASE 
    WHEN Type = 130 THEN 'sys.SingleQuote(' | Name | ')'
    ELSE Name
  END
  FROM sys.Column WHERE Table = table
    SET result |= '|'',''|' | col
  RETURN result
END
GO
CREATE FUNCTION [sys].[ColNames]( table int ) RETURNS string AS
BEGIN
  DECLARE col string
  SET result = '(Id'
  FOR col = Name FROM sys.Column WHERE Table = table
    SET result |= ',' | sys.QuoteName(col)
  RETURN result | ')'
END
GO
CREATE FUNCTION [sys].[ColName]( table int, colId int ) RETURNS string AS
BEGIN
  DECLARE i int
  SET i = 0
  FOR result = Name FROM sys.Column WHERE Table = table
  BEGIN
    IF i = colId RETURN result
    SET i = i + 1
  END
  RETURN '?bad colId?'  
END
GO
--############################################
CREATE SCHEMA [date]
CREATE FUNCTION [date].[YearMonthDayToYearDay]( ymd int ) RETURNS int AS
BEGIN
  DECLARE y int, m int, d int
  -- Extract y, m, d from ymd
  SET d = ymd % 32, m = ymd / 32  
  SET y = m / 16, m = m % 16
  -- Incorporate m into d ( assuming Feb has 29 days ).
  SET d = d + CASE
    WHEN m = 1 THEN 0 -- Jan
    WHEN m = 2 THEN 31 -- Feb
    WHEN m = 3 THEN 60 -- Mar
    WHEN m = 4 THEN 91 -- Apr
    WHEN m = 5 THEN 121 -- May
    WHEN m = 6 THEN 152 -- Jun
    WHEN m = 7 THEN 182 -- Jul
    WHEN m = 8 THEN 213 -- Aug
    WHEN m = 9 THEN 244 -- Sep
    WHEN m = 10 THEN 274 -- Oct
    WHEN m = 11 THEN 305 -- Nov
    ELSE 335 -- Dec
    END
  -- Allow for Feb being only 28 days in a non-leap-year.
  IF m >= 3 AND NOT date.IsLeapYear( y ) SET d = d - 1
  RETURN date.YearDay( y, d )
END
GO
CREATE FUNCTION [date].[YearMonthDayToString]( ymd int ) RETURNS string AS
BEGIN
  DECLARE y int, m int, d int
  SET d = ymd % 32
  SET m = ymd / 32
  SET y = m / 16
  SET m = m % 16
  RETURN date.MonthToString(m) | ' ' | d | ' ' |  y
END
GO
CREATE FUNCTION [date].[YearMonthDayToDays]( ymd int ) RETURNS int AS
BEGIN
  RETURN date.YearDayToDays( date.YearMonthDayToYearDay( ymd ) )
END
GO
CREATE FUNCTION [date].[YearMonthDay]( year int, month int, day int ) RETURNS int AS
BEGIN
  RETURN year * 512 + month * 32 + day
END
GO
CREATE FUNCTION [date].[YearDayToYearMonthDay]( yd int ) RETURNS int AS
BEGIN
  DECLARE y int, d int, leap bool, fdm int, m int, dim int
  SET y = yd / 512
  SET d = yd % 512 - 1
  SET leap = date.IsLeapYear( y )
  -- Jan = 0..30, Feb = 0..27 or 0..28  
  IF NOT leap AND d >= 59 SET d = d + 1
  SET fdm = CASE 
    WHEN d < 31 THEN 0 -- Jan
    WHEN d < 60 THEN 31 -- Feb
    WHEN d < 91 THEN 60 -- Mar
    WHEN d < 121 THEN 91 -- Apr
    WHEN d < 152 THEN 121 -- May
    WHEN d < 182 THEN 152 -- Jun
    WHEN d < 213 THEN 182 -- Jul
    WHEN d < 244 THEN 213 -- Aug
    WHEN d < 274 THEN 244 -- Sep
    WHEN d < 305 THEN 274 -- Oct
    WHEN d < 335 THEN 305 -- Nov
    ELSE 335 -- Dec
    END
  SET dim = d - fdm
  SET m = ( d - dim + 28 ) / 31
  RETURN date.YearMonthDay( y, m+1, dim+1 )
END
GO
CREATE FUNCTION [date].[YearDayToString]( yd int ) RETURNS string AS
BEGIN
   RETURN date.YearMonthDayToString( date.YearDayToYearMonthDay( yd ) )  
END
GO
CREATE FUNCTION [date].[YearDayToDays]( yd int ) RETURNS int AS
BEGIN
  -- Given a date in Year/Day representation stored as y * 512 + d where 1 <= d <= 366 ( so d is day in year )
  -- returns the number of days since \"day zero\" (1 Jan 0000)
  -- using the Gregorian calendar where days divisible by 4 are leap years, except if divisible by 100, except if divisible by 400.
  DECLARE y int, d int, cycle int
  -- Extract y and d from yd.
  SET y = yd / 512, d = yd % 512 - 1
  SET cycle = y / 400, y = y % 400 -- The Gregorian calendar repeats every 400 years.
 
  -- Result days come from cycles, from years having at least 365 days, from leap years and finally d.
  -- 146097 is the number of the days in a 400 year cycle ( 400 * 365 + 97 leap years ).
  RETURN cycle * 146097 
    + y * 365 
    + ( y + 3 ) / 4 - ( y + 99 ) / 100 + ( y + 399 ) / 400
    + d
END
GO
CREATE FUNCTION [date].[YearDay]( year int, day int ) RETURNS int AS
BEGIN
  RETURN year * 512 + day
END
GO
CREATE FUNCTION [date].[WeekDayToString]( wd int ) RETURNS string AS
BEGIN
  RETURN CASE
    WHEN wd = 1 THEN 'Mon'
    WHEN wd = 2 THEN 'Tue'
    WHEN wd = 3 THEN 'Wed'
    WHEN wd = 4 THEN 'Thu'
    WHEN wd = 5 THEN 'Fri'
    WHEN wd = 6 THEN 'Sat'
    WHEN wd = 7 THEN 'Sun'
    ELSE '?weekday?'
    END
END
GO
CREATE FUNCTION [date].[Today]() RETURNS int AS
BEGIN
  DECLARE sec int, day int
  SET sec = date.Ticks() / 1000000
  SET day = sec / 86400 + 366
  RETURN day
END
GO
CREATE FUNCTION [date].[Ticks]() RETURNS int AS
BEGIN
  -- Microseconds since 1 Jan 0000
  RETURN GLOBAL(0) + 62135596800000000 /* 719162 * 24 * 3600 * 1000000 */
END
GO
CREATE FUNCTION [date].[Test]( y int, m int, d int, n int ) AS 
BEGIN
  DECLARE ymd int, days int
  SET ymd = date.YearMonthDay( y, m, d )
  SET days = date.YearMonthDayToDays( ymd )
  DECLARE i int
  SET i = 0
  WHILE i < n
  BEGIN
    SELECT '<br>' | date.DaysToString( days + i )
    SET i = i + 1
  END
END
GO
CREATE FUNCTION [date].[StringToYearMonthDay]( s string ) RETURNS int AS
BEGIN
  RETURN date.DaysToYearMonthDay( date.StringToDays( s ) )
END
GO
CREATE FUNCTION [date].[StringToDays]( s string ) RETURNS int AS
BEGIN
  -- Typical input is 'Feb 2 2020'
  DECLARE ms string, month int
  SET ms = SUBSTRING( s, 1, 3 )
  SET month = CASE 
    WHEN ms = 'Jan' THEN 1
    WHEN ms = 'Feb' THEN 2
    WHEN ms = 'Mar' THEN 3
    WHEN ms = 'Apr' THEN 4
    WHEN ms = 'May' THEN 5
    WHEN ms = 'Jun' THEN 6
    WHEN ms = 'Jul' THEN 7
    WHEN ms = 'Aug' THEN 8
    WHEN ms = 'Sep' THEN 9
    WHEN ms = 'Oct' THEN 10
    WHEN ms = 'Nov' THEN 11
    WHEN ms = 'Dec' THEN 12
    ELSE 0
  END  
  IF month = 0 THROW 'Unknown month parsing date ' | htm.Attr(ms)
  DECLARE six int -- Index of first space
  SET six = 4
  WHILE true
  BEGIN
    IF six > LEN(s) BREAK
    IF SUBSTRING( s, six, 1 ) = ' ' BREAK
    SET six = six + 1
  END
  DECLARE ssix int
  SET ssix = six+1
  WHILE true
  BEGIN
    IF ssix > LEN(s) BREAK
    IF SUBSTRING( s, ssix, 1 ) = ' ' BREAK
    SET ssix = ssix + 1
  END
 
  DECLARE day int, year int
  SET day = PARSEINT( SUBSTRING( s, six+1, ssix - six - 1) )
  IF day < 1 OR day > 31 THROW 'Day must be 1..31 parsing date ' | htm.Attr(day)
  SET year = PARSEINT( SUBSTRING( s, ssix + 1, LEN(s) ) )
  RETURN date.YearMonthDayToDays( date.YearMonthDay( year, month, day ) )
END
GO
CREATE FUNCTION [date].[NowString]() RETURNS string AS
BEGIN
  DECLARE day int, sec int, min int, hour int
  SET sec = date.Ticks() / 1000000
  SET day = sec / 86400 + 366 -- 86400 = 24 * 60 * 60, seconds in a day.
  SET sec = sec % 86400
  SET min = sec / 60
  SET sec = sec % 60
  SET hour = min / 60
  SET min = min % 60
  RETURN date.DaysToString(  day ) | ' ' | hour | ':' | min | ':' | sec
END
GO
CREATE FUNCTION [date].[MonthToString]( m int ) RETURNS string AS
BEGIN
  RETURN CASE
    WHEN m = 1 THEN 'Jan'
    WHEN m = 2 THEN 'Feb'
    WHEN m = 3 THEN 'Mar'
    WHEN m = 4 THEN 'Apr'
    WHEN m = 5 THEN 'May'
    WHEN m = 6 THEN 'Jun'
    WHEN m = 7 THEN 'Jul'
    WHEN m = 8 THEN 'Aug'
    WHEN m = 9 THEN 'Sep'
    WHEN m = 10 THEN 'Oct'
    WHEN m = 11 THEN 'Nov'
    WHEN m = 12 THEN 'Dec'
    ELSE '???'
  END
END
GO
CREATE FUNCTION [date].[IsLeapYear]( y int ) RETURNS bool AS
BEGIN
  RETURN y % 4 = 0 AND ( y % 100 != 0 OR y % 400 = 0 )
END
GO
CREATE FUNCTION [date].[DaysToYearMonthDay]( days int ) RETURNS int AS
BEGIN
  RETURN date.YearDayToYearMonthDay( date.DaysToYearDay( days ) )
END
GO
CREATE FUNCTION [date].[DaysToYearDay]( days int ) RETURNS int AS
BEGIN
  -- Given a date represented by the number of days since 1 Jan 0000
  -- calculate a date in Year/Day representation stored as
  -- year * 512 + day where day is 1..366, the day in the year.
  
  DECLARE year int, day int, cycle int
  -- 146097 is the number of the days in a 400 year cycle ( 400 * 365 + 97 leap years )
  SET cycle = days / 146097
  SET days = days % 146097
  SET year = days / 365
  SET day = days % 365
  -- Need to adjust day to allow for leap years.
  -- Leap years are 0, 4, 8, 12 ... 96, not 100, 104 ... not 200... not 300, 400, 404 ... not 500.
  -- Adjustment as function of y is 0 => 0, 1 => 1, 2 =>1, 3 => 1, 4 => 1, 5 => 2 ..
  SET day = day - ( year + 3 ) / 4 - ( year + 99 ) / 100 + ( year + 399 ) / 400
  
  IF day < 0
  BEGIN
    SET year = year - 1
    SET day = day + CASE WHEN date.IsLeapYear( day ) THEN 366 ELSE 365 END
  END
  RETURN 512 * ( cycle * 400 + year ) + day + 1
END
GO
CREATE FUNCTION [date].[DaysToString]( date int ) RETURNS string AS
BEGIN
  RETURN date.WeekDayToString( 1 + (date+5) % 7 ) | ' ' | date.YearMonthDayToString( date.DaysToYearMonthDay( date ) )
END
GO
--############################################
CREATE SCHEMA [htm]
CREATE FUNCTION [htm].[Encode]( s string ) RETURNS string AS
BEGIN
  SET s = REPLACE( s,'&', '&amp;' )
  SET s = REPLACE( s, '<', '&lt;' )
  RETURN s
END
GO
CREATE FUNCTION [htm].[Attr]( s string ) RETURNS string AS
BEGIN
  SET s = REPLACE( s, '&', '&amp;' )
  SET s = REPLACE( s, '\"', '&quot;' )
  RETURN '\"' | s | '\"'
END
GO
--############################################
CREATE SCHEMA [web]
CREATE TABLE [web].[File]([Path] string,[ContentType] string,[ContentLength] int,[Content] binary) 
GO
CREATE FUNCTION [web].[Trailer]() AS
BEGIN
  SELECT '</body></html>'
END
GO
CREATE FUNCTION [web].[SetCookie]( name string, value string, expires string ) AS
BEGIN
  -- SELECT 16, name, value, expires /* e.g. 01 Jan 2050 */
  THROW 'SetCookie is ToDo'
END
GO
CREATE FUNCTION [web].[SetContentType]( ct string ) AS
BEGIN
  DECLARE dummy string
  SET dummy = ARG( 10, 'ContentType: ' | ct )
END
GO
CREATE FUNCTION [web].[SendBinary]( contenttype string, content binary ) AS
BEGIN
  EXEC web.SetContentType( contenttype )
  SELECT 11, content
END
GO
CREATE FUNCTION [web].[Redirect]( url string ) AS
BEGIN
  DECLARE dummy string
  SET dummy = ARG( 10, 'Location: ' | url )
  SET dummy = ARG( 11, '303 Redirect' )
END
GO
CREATE FUNCTION [web].[Query]( name string ) RETURNS string AS
BEGIN
  RETURN ARG( 1, name )
END
GO
CREATE FUNCTION [web].[Path]() RETURNS string AS
BEGIN
  RETURN ARG(0,'')
END
GO
CREATE FUNCTION [web].[Main]() AS 
BEGIN 
  DECLARE path string SET path = web.Path()
  DECLARE ok string SET ok = Name FROM sys.Procedure WHERE Name = path AND Schema = 2
  IF ok = path
  BEGIN
    EXECUTE( 'EXEC ' | sys.Dot('handler',path) | '()' )
    DECLARE ex string
    SET ex = EXCEPTION()
    IF ex != ''
    BEGIN
      EXEC web.Head( 'Error' )
      SELECT '<h1>Error</h1><pre>'
      SELECT htm.Encode( ex )
      SELECT '</pre>'
      EXEC web.Trailer()
    END
  END
  ELSE
  BEGIN
    DECLARE ct string, content binary
    SET ok = Path, ct = ContentType, content = Content FROM web.File WHERE Path = path
    IF ok = path
    BEGIN
      EXEC web.SendBinary( ct, content )
    END    
    ELSE
    BEGIN
      EXEC web.Head( 'Unknown page')
      SELECT 'Unknown page Path=' | path
      EXEC web.Trailer()
    END
  END
END
GO
CREATE FUNCTION [web].[Head]( title string ) AS 
BEGIN 
  EXEC web.SetContentType( 'text/html;charset=utf-8' )
  SELECT '<html>
<head>
<meta http-equiv=\"Content-type\" content=\"text/html;charset=UTF-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>' | title | '</title>
<style>
   body{font-family:sans-serif;}
   body{ max-width:60em; }
</style>
</head>
<body>
<div style=\"color:white;background:lightblue;padding:4px;\">
<a href=/Menu>Menu</a> 
| <a target=_blank href=/Menu>New Window</a>
| <a href=Manual>Manual</a>
| <a target=_blank href=\"EditFunc?s=handler&n=' | web.Path() | '\">Code</a> ' | date.NowString() | ' UTC</div>'
END
GO
CREATE FUNCTION [web].[Form]( name string ) RETURNS string AS
BEGIN
  RETURN ARG( 2, name )
END
GO
CREATE FUNCTION [web].[Cookie]( name string ) RETURNS string AS
BEGIN
  RETURN ARG( 3, name )
END
GO
INSERT INTO [web].[File](Id,[Path],[ContentType],[ContentLength],[Content]) VALUES 
GO

--############################################
CREATE SCHEMA [browse]
CREATE TABLE [browse].[Column]([Position] int,[Label] string,[Description] string,[RefersTo] int,[Default] string,[InputCols] int,[InputFunction] string,[InputRows] int,[Style] int,[DisplayFunction] string,[ParseFunction] string) 
GO
CREATE INDEX [ByRefersTo] ON [browse].[Column]([RefersTo])
GO
CREATE TABLE [browse].[Table]([NameFunction] string,[SelectFunction] string,[DefaultOrder] string,[Title] string,[Description] string,[Role] int) 
GO
CREATE FUNCTION [browse].[UpdateSql]( table int, k int ) RETURNS string AS
BEGIN
  DECLARE alist string, col string, type int, colId int
  FOR colId = Id, col = Name, type = Type FROM sys.Column WHERE Table = table
  BEGIN
    DECLARE f string SET f = 'web.Form(' | sys.SingleQuote(col) | ')'
    SET alist |= CASE WHEN alist = '' THEN '' ELSE ' , ' END
      | sys.QuoteName(col) | ' = ' | browse.ColParser( colId, type, f )
  END
  RETURN 'UPDATE ' | sys.TableName( table ) | ' SET ' | alist | ' WHERE Id =' | k
END
GO
CREATE FUNCTION [browse].[TableTitle]( table int ) RETURNS string AS
BEGIN
  SET result = Title FROM browse.Table WHERE Id = table
  IF result = '' SET result = Name FROM sys.Table WHERE Id = table
END
GO
CREATE FUNCTION [browse].[TableSelect]( colId int, sel int ) RETURNS string AS
BEGIN
  DECLARE col string SET col = Name FROM sys.Column WHERE Id = colId
  DECLARE opt string, options string
  FOR opt = '<option ' | CASE WHEN Id = sel THEN ' selected' ELSE '' END 
  | ' value=' | Id | '>' | htm.Encode( sys.TableName(Id) ) | '</option>'
  FROM sys.Table
  ORDER BY sys.TableName(Id)
  SET options |= opt
  RETURN '<select id=\"' | col | '\" name=\"' | col | '\">' | options | 
     '<option ' | CASE WHEN sel = 0 THEN ' selected' ELSE '' END | ' value=0></option>'
     | '</select>'
END
GO
CREATE FUNCTION [browse].[ShowSql]( table int, k int ) RETURNS string AS
BEGIN
  DECLARE cols string, col string, colname string, colid int
  FOR colid = Id, colname = Name, col = CASE 
    WHEN Type = 130 THEN 'htm.Encode(' | Name | ')'
    ELSE Name
    END
  FROM sys.Column WHERE Table = table 
  ORDER BY browse.ColPos(Id), Id
  BEGIN
    DECLARE ref int, nf string, df string
    SET ref = 0, nf = '', df = ''
    SET ref = RefersTo, df = DisplayFunction FROM browse.Column WHERE Id = colid
    IF ref > 0 SET nf = NameFunction FROM browse.Table WHERE Id = ref ELSE SET nf = ''
    SET cols |= 
      CASE WHEN cols = '' THEN '' ELSE ' | ' END
      | '''<p>' | colname | ': '' | '
      | CASE 
        WHEN df != '' THEN df | '(' | col | ')'
        WHEN nf != '' THEN '''<a href=\"/ShowRow?t=' | ref | '&k=''|' | col | '|''\">''|' | nf | '(' | col | ')' | '|''</a>''' 
        ELSE col
        END
  END
  DECLARE namefunc string SET namefunc = NameFunction FROM browse.Table WHERE Id = table
  RETURN '  
    DECLARE t int SET t = '|table|'
    DECLARE k int SET k = '|k|'
    DECLARE title string SET title = browse.TableTitle( t )' 
      | CASE WHEN namefunc = '' THEN '' ELSE ' | '' '' | ' | namefunc | '(k)' END | '
    EXEC web.Head( title )
    SELECT ''<b>'' | title | ''</b><br>''
  '
  | ' SELECT ' | cols | ' FROM ' | sys.TableName(table) | ' WHERE Id = k'
  | ' SELECT ''<p><a href=\"/EditRow?t='' | t | ''&k='' | k | ''\">Edit</a>'''
  | '
  DECLARE col int
  FOR col = Id FROM browse.Column WHERE RefersTo = t
  BEGIN
    SELECT ''<p><b>'' | browse.TableTitle( Table ) | ''</b>''
     | '' <a href=\"AddChild?c='' | col | ''&p='' | k | ''\">Add</a>''
    FROM sys.Column WHERE Id = col
    EXECUTE( browse.ChildSql( col, k ) )
  END
  SELECT ''<p><a href=\"/ShowTable?k='' | t | ''\">'' | browse.TableTitle(t) | '' Table</a>''
  EXEC web.Trailer()
'
END
GO
CREATE FUNCTION [browse].[SchemaSelect]( colId int, sel int ) RETURNS string AS
BEGIN
  DECLARE col string SET col = Name FROM sys.Column WHERE Id = colId
  DECLARE opt string, options string, sels string
  SET sels = web.Form( col )
  IF sels != '' SET sel = PARSEINT( sels )
  FOR opt = '<option ' | CASE WHEN Id = sel THEN ' selected' ELSE '' END 
  | ' value=' | Id | '>' | htm.Encode( Name ) | '</option>'
  FROM sys.Schema
  ORDER BY Name
  SET options |= opt
  RETURN '<select id=\"' | col | '\" name=\"' | col | '\">' | options | 
     '<option ' | CASE WHEN sel = 0 THEN ' selected' ELSE '' END | ' value=0></option>'
     | '</select>'
END
GO
CREATE FUNCTION [browse].[ParseBool]( s string ) RETURNS bool AS
BEGIN
  RETURN s = 'on'
END
GO
CREATE FUNCTION [browse].[InsertSql]( table int, pc int, p int ) RETURNS string AS
BEGIN
  DECLARE vlist string, f string, type int, colId int
  FOR f = 'web.Form(' | sys.SingleQuote(Name) | ')', type = Type, colId = Id
  FROM sys.Column WHERE Table = table 
  SET vlist |= CASE WHEN vlist = '' THEN '' ELSE ' , ' END | 
    CASE 
    WHEN colId = pc THEN '' | p
    ELSE browse.ColParser( colId, type, f )
    END
  RETURN 'INSERT INTO ' | sys.TableName( table ) | browse.InsertNames( table ) | ' VALUES (' | vlist | ')'
END
GO
CREATE FUNCTION [browse].[InsertNames]( table int ) RETURNS string AS
BEGIN
  DECLARE col string
  FOR col = Name FROM sys.Column WHERE Table = table
    SET result |= CASE WHEN result = '' THEN '' ELSE ',' END | sys.QuoteName(col)
  RETURN '(' | result | ')'
END
GO
CREATE FUNCTION [browse].[InputYearMonthDay]( colId int, value int) RETURNS string AS 
BEGIN 
  DECLARE cn string 
  SET cn = Name FROM sys.Column WHERE Id = colId
  DECLARE size int
  SET size = InputCols FROM browse.Column WHERE Id = colId
  IF size = 0 SET size = 10
  RETURN '<input id=\"' | cn | '\" name=\"' | cn | '\" size=' | size | ' value=' | htm.Attr(date.YearMonthDayToString(value)) | '>'
END
GO
CREATE FUNCTION [browse].[InputString]( colId int, value string ) RETURNS string AS 
BEGIN 
  DECLARE cn string SET cn = Name FROM sys.Column WHERE Id = colId 
  DECLARE cols int, rows int, description string
  SET cols = InputCols, rows = InputRows, description = Description
  FROM browse.Column WHERE Id = colId
  IF cols = 0 SET cols = 50
  IF rows > 0
    RETURN '<textarea id=\"' | cn | '\" name=\"' | cn | '\" cols=\"' | cols | '\"' | '\" rows=\"' | rows |'\"'
      | CASE WHEN value = '' THEN 'placeholder=' | htm.Attr(description) ELSE '' END
      | '\">' | htm.Encode(value) | '</textarea>'
  ELSE
    RETURN '<input id=\"' | cn | '\" name=\"' | cn | '\" size=\"' | cols | '\"' | ' value=' | htm.Attr(value) | '>'
END
GO
CREATE FUNCTION [browse].[InputInt]( colId int, value int) RETURNS string AS 
BEGIN 
  DECLARE cn string 
  SET cn = Name FROM sys.Column WHERE Id = colId
  DECLARE size int
  SET size = InputCols FROM browse.Column WHERE Id = colId
  IF size = 0 SET size = 10
  RETURN '<input type=number id=\"' | cn | '\" name=\"' | cn | '\" size=' | size | ' value=' | value | '>'
END
GO
CREATE FUNCTION [browse].[InputDouble]( colId int, value double ) RETURNS string AS 
BEGIN 
  DECLARE cn string SET cn = Name FROM sys.Column WHERE Id = colId
  DECLARE size int 
  SET size = InputCols FROM browse.Column WHERE Id = colId
  IF size = 0 SET size = 15
  RETURN '<input id=\"' | cn | '\" name=\"' | cn | '\" size=\"' | size | '\"' | ' value=\"' | value | '\">'
END
GO
CREATE FUNCTION [browse].[InputDecimal]( colId int, value decimal(10,2) ) RETURNS string AS 
BEGIN 
  DECLARE cn string SET cn = Name FROM sys.Column WHERE Id = colId 
  DECLARE cols int, description string
  SET cols = InputCols, description = Description
  FROM browse.Column WHERE Id = colId
  IF cols = 0 SET cols = 50
  RETURN '<input id=\"' | cn | '\" name=\"' | cn | '\" size=\"' | cols | '\"' | ' value=' | htm.Attr(''|value) | '>'
END
GO
CREATE FUNCTION [browse].[InputBool]( colId int, value bool ) RETURNS string AS
BEGIN
  DECLARE cn string 
  SET cn = Name FROM sys.Column WHERE Id = colId
  RETURN '<input type=checkbox id=\"' | cn | '\" name=\"' | cn | '\"' | CASE WHEN value THEN ' checked' ELSE '' END | '>'
END
GO
CREATE FUNCTION [browse].[InputBinary]( colId int, value binary ) RETURNS string AS 
BEGIN 
  DECLARE cn string SET cn = Name FROM sys.Column WHERE Id = colId
  DECLARE size int SET size = InputCols FROM browse.Column WHERE Id = colId
  IF size = 0 SET size = 50
  RETURN '<input id=\"' | cn | '\" name=\"' | cn | '\" size=' | size | ' value=\"' | value | '\">'
END
GO
CREATE FUNCTION [browse].[FormUpdateSql]( table int, k int ) RETURNS string AS
BEGIN
  DECLARE sql string, col string, colId int, type int
  FOR col = Name, colId = Id, type = Type FROM sys.Column WHERE Table = table
  ORDER BY browse.ColPos(Id), Id
  BEGIN
    DECLARE ref int, inf string
    SET ref = 0, inf = ''
    SET ref = RefersTo, inf = InputFunction FROM browse.Column WHERE Id = colId
    IF ref > 0 AND inf = '' SET inf = SelectFunction FROM browse.Table WHERE Id = ref
    IF inf = '' SET inf = browse.DefaultInput( type )
    SET sql |= 
      CASE WHEN sql = '' THEN '' ELSE ' | ' END
      | '''<p><label for=\"' | col | '\">' | col | '</label>: '' | ' 
      | inf | '(' | colId | ',' | sys.QuoteName(col) | ')'
  END
  RETURN 'SELECT ' | sql | ' FROM ' | sys.TableName( table ) | ' WHERE Id =' | k
END
GO
CREATE FUNCTION [browse].[FormInsertSql]( table int, pc int ) RETURNS string AS
BEGIN
  DECLARE sql string, col string, type int, colId int
  FOR col = Name, type = Type, colId = Id FROM sys.Column 
    WHERE Table = table AND Id != pc
    ORDER BY browse.ColPos(Id), Id
  BEGIN
    DECLARE ref int, inf string, default string
    SET ref = 0, inf = '', default = ''
    SET ref = RefersTo,  inf = InputFunction, default = Default FROM browse.Column WHERE Id = colId
    IF ref > 0 AND inf = '' SET inf = SelectFunction FROM browse.Table WHERE Id = ref
    IF inf = '' SET inf = browse.DefaultInput( type )
    IF default = '' SET default = browse.DefaultDefault( type, ref )
 
    SET sql |= CASE WHEN sql = '' THEN '' ELSE ' | ' END
      | '''<p><label for=\"' | col | '\">' | col | '</label>: '' | ' 
      | inf | '(' | colId | ',' | default | ')'
  END
  RETURN 'SELECT ' | sql
END
GO
CREATE FUNCTION [browse].[DefaultInput]( type int ) RETURNS string AS
BEGIN
  RETURN CASE 
  WHEN type % 8 = 3 THEN 'browse.InputInt'
  WHEN type % 8 = 1 THEN 'browse.InputBinary'
  WHEN type % 8 = 4 THEN 'browse.InputDouble'
  WHEN type % 8 = 5 THEN 'browse.InputBool'
  WHEN type % 8 = 6 THEN 'browse.InputDecimal'
  ELSE 'browse.InputString'
  END
END
GO
CREATE FUNCTION [browse].[DefaultDefault]( type int, ref int ) RETURNS string AS
BEGIN
  RETURN CASE
    WHEN type % 8 = 2 THEN ''''''
    WHEN type % 8 = 1 THEN '0x'
    WHEN type % 8 = 5 THEN 'false'
    ELSE '0'
    END
END
GO
CREATE FUNCTION [browse].[ColValues]( table int ) RETURNS string AS
BEGIN
  DECLARE col string, colid int
  FOR colid = Id, col = CASE 
    WHEN Type % 8 = 2 THEN 'htm.Encode(sys.SingleQuote(' | Name | '))'
    ELSE Name
  END
  FROM sys.Column WHERE Table = table 
  ORDER BY browse.ColPos(Id), Id
  BEGIN
    DECLARE ref int, nf string, df string
    SET ref = 0, nf = '', df = ''
    SET ref = RefersTo, df = DisplayFunction FROM browse.Column WHERE Id = colid
    IF ref > 0 SET nf = NameFunction FROM browse.Table WHERE Id = ref
    SET result |= CASE WHEN result = '' THEN '' ELSE '|'', ''|' END | 
      CASE 
      WHEN df != '' THEN df | '(' | col | ')'
      WHEN nf != '' 
      THEN '''<a href=\"/ShowRow?t=' | ref | '&k=''|' | col | '|''\">''|' | nf | '(' | col | ')' | '|''</a>''' 
      ELSE col
      END
  END
END
GO
CREATE FUNCTION [browse].[ColPos]( c int ) RETURNS int AS
BEGIN
  DECLARE pos int
  SET pos = Position FROM browse.Column WHERE Id = c
  RETURN pos
END
GO
CREATE FUNCTION [browse].[ColParser]( colId int, type int, f string ) RETURNS string AS
BEGIN
  -- ColId not currently used, but in future user-specified parser could be fetched from Parse.Column
  DECLARE pf string
  SET pf = ParseFunction FROM browse.Column WHERE Id = colId
  RETURN CASE 
    WHEN pf != '' THEN pf | '(' | f | ')'
    WHEN type % 8 = 3 THEN 'PARSEINT(' | f |')'
    WHEN type % 8 = 4 THEN 'PARSEFLOAT(' | f | ')'
    WHEN type % 8 = 5 THEN 'browse.ParseBool(' | f | ')'
    WHEN type % 8 = 6 THEN 'PARSEDECIMAL(' | f | ',' | type | ')'
    ELSE f
  END
END
GO
CREATE FUNCTION [browse].[ColNames]( table int ) RETURNS string AS
BEGIN
  DECLARE col string
  FOR col = '<a href=\"/BrowseColInfo?k=' | Id | '\">' | Name | '</a>' 
    | ' ' | sys.TypeName(Type) /* | ' pos=' | browse.ColPos(Id) */
  FROM sys.Column WHERE Table = table
  ORDER BY browse.ColPos(Id), Id
  BEGIN
    SET result |= CASE WHEN result = '' THEN '' ELSE ', ' END | col
  END
END
GO
CREATE FUNCTION [browse].[ChildSql]( colId int, k int ) RETURNS string AS 
BEGIN 
  /* Returns SQL to display a child table, with hyperlinks where a column refers to another table */
  DECLARE col string, colid int, colName string, type int, th string, ob string
  DECLARE table int SET table = Table FROM sys.Column WHERE Id = colId
  
  SET ob = DefaultOrder FROM browse.Table WHERE Id = table
  FOR colid = Id, type = Type,
    col = CASE WHEN Type = 2 THEN 'htm.Encode(' | Name | ')' ELSE Name END, colName = Name
  FROM sys.Column WHERE Table = table AND Id != colId
  ORDER BY browse.ColPos(Id), Id
  BEGIN
    DECLARE ref int, nf string, label string, df string
    SET ref = 0, nf = '', df = ''
    SET ref = RefersTo, label = Label, df = DisplayFunction FROM browse.Column WHERE Id = colid
    IF ref > 0 SET nf = NameFunction FROM browse.Table WHERE Id = ref
    SET ob = DefaultOrder FROM browse.Table WHERE Id = ref
    SET result |= '|''<TD' | CASE WHEN type != 2 THEN ' align=right' ELSE '' END | '>''|'
      | CASE 
        WHEN df != '' THEN df | '(' | col | ')'
        WHEN nf != '' 
        THEN '''<a href=\"/ShowRow?t=' | ref | '&k=''|' | col | '|''\">''|' | nf | '(' | col | ')' | '|''</a>''' 
        ELSE col
        END,
        th = th | '<TH>' | CASE WHEN label != '' THEN label ELSE colName END
  END
  DECLARE kcol string SET kcol = sys.QuoteName(Name) FROM sys.Column WHERE Id = colId
  RETURN 
   'SELECT ''<TABLE><TR><TH>' | th | ''' '
   | 'SELECT ' | '''<TR><TD><a href=\"ShowRow?t=' | table | '&k=''| Id | ''\">Show</a> '''
     | result | ' FROM ' | sys.TableName( table ) | ' WHERE ' | kcol | ' = ' | k | CASE WHEN ob != '' THEN ' ORDER BY ' | ob ELSE '' END
   | ' SELECT ''</TABLE>'''
END
GO
CREATE FUNCTION [browse].[BrowseColumnName]( k int ) RETURNS string AS 
BEGIN
  SET result = sys.TableName( Table ) | '.' | sys.QuoteName( Name )
  FROM sys.Column WHERE Id = k
END
GO
--############################################
CREATE SCHEMA [handler]
CREATE FUNCTION [handler].[/ShowTable]() AS 
BEGIN 
  DECLARE t int SET t = PARSEINT( web.Query('k') )
  DECLARE title string SET title = browse.TableTitle( t )
  SET title = title | ' Table'
  EXEC web.Head( title )
  SELECT '<b>' | title | '</b> <a href=/BrowseInfo?k=' | t | '>Settings</a>'   
    | '<p><b>Columns:</b> ' | browse.ColNames( t )
/*
  SELECT '<p><b>Indexes</b>'
  SELECT '<br>' | sys.QuoteName(Name) | ' ' | sys.IndexCols(Id)
  FROM sys.Index WHERE Table = t
*/
  SELECT '<p><b>Rows</b> <a href=\"AddRow?t=' | t | '\">Add</a>'
  
  DECLARE orderBy string SET orderBy = DefaultOrder FROM browse.Table WHERE Id = t
  DECLARE sql string SET sql ='SELECT ''<br><a href=\"ShowRow?t=' | t | '&k=''| Id |''\">Show</a> ''| ''''|' 
    | browse.ColValues(Id)  
    | ' FROM ' 
    | sys.TableName(Id)
    | CASE WHEN orderBy != '' THEN ' ORDER BY ' | orderBy ELSE '' END
  FROM sys.Table WHERE Id = t
  EXECUTE( sql )
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/ShowSchema]() AS
BEGIN
  DECLARE s string SET s = web.Query('s')
  DECLARE sid int SET sid = Id FROM sys.Schema WHERE Name = s
  EXEC web.Head( 'Schema ' | s )
  SELECT '<h1>Schema ' | s | '</h1>'
  SELECT '<h2>Tables</h2>'
  SELECT '<p><a href=\"ShowTable?k=' | Id | '\">' | Name | '</a>'
  FROM sys.Table WHERE Schema = sid AND IsView = 0 ORDER BY Name
/*
  SELECT '<h2>Views</h2>'
  SELECT '<p><a href=\"EditView?s=' | s | '&n=' | Name | '\">' | Name | '</a>'
  FROM sys.Table WHERE Schema = sid AND IsView = 1 ORDER BY Name
*/
  SELECT '<h2>Functions</h2>' 
  SELECT '<p><a href=\"EditFunc?s=' | s | '&n=' | Name | '\">' | Name | '</a>'
  FROM sys.Function WHERE Schema = sid ORDER BY Name
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/ShowRow]() AS 
BEGIN
  DECLARE t int SET t = PARSEINT( web.Query('t') )
  DECLARE k int SET k = PARSEINT( web.Query('k') )
  EXECUTE( browse.ShowSql( t, k ) )
END
GO
CREATE FUNCTION [handler].[/OrderSummary]() AS
BEGIN
  EXEC web.Head( 'Order Summary' )
  SELECT '<table><tr><th>Cust<th>Total<th>#<th>Avg<th>Min<th>Max</tr>'
  SELECT '<tr><td><a href=ShowRow?t=11&k=' | Cust | '>' | dbo.CustName(Cust) | '</a>' 
   | '<td align=right>' | Total
   | '<td align=right>' | Count
   | '<td align=right>' | Total / Count
   | '<td align=right>' | Min
   | '<td align=right>' | Max
   | '</tr>'
  FROM dbo.OrderSummary
  ORDER BY Total / Count DESC
  SELECT '</table>'
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/Menu]() AS
BEGIN
   EXEC web.Head('Menu')
   SELECT '
<p><a href=\"/ShowTable?k=10\">Customers</a>
<p><a href=/OrderSummary>Order Summary</a>
<h1>System</h1>
<p><a href=/Execute>Execute SQL</a>
<p><a href=/ListFile>Files</a>
<p><a href=/FileUpload>File Upload</a>
<p><a target=_blank href=/Dump>Dump</a>
<h1>Schemas</h1>'
   SELECT '<p><a href=ShowSchema?s=' | Name | '>' | Name | '</a>' FROM sys.Schema ORDER BY Name
   EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/Manual]() AS BEGIN
EXEC web.Head('Manual')
SELECT '<h1>Manual</h1>
<p>This manual describes the various SQL statements that are available. Where syntax is described, optional elements are enclosed in square brackets.
<h2>Schema definition</h2>
<h3>CREATE SCHEMA</h3>
<p>CREATE SCHEMA name
<p>Creates a new schema. Every database object (Table,View,Procedure,Function) has an associated schema. Schemas are used to organise database objects into logical categories.
<h2>Table definition</h2>
<h3>CREATE TABLE</h3><p>CREATE TABLE schema.tablename ( Colname1 Coltype1, Colname2 Coltype2, ... )
<p>Creates a new base table. Every base table is automatically given an Id column, which auto-increments on INSERT ( if no explicit value is supplied).<p>The data types are as follows:
<ul>
<li>tinyint, smallint, int, bigint : signed integers of size 1, 2, 4 and 8 bytes respectively.</li>
<li>float, double : floating point numbers of size 4 and 8 bytes respectively.</li>
<li>decimal(p,s) : a number with p decimal digits, with s digits after the decimal point. The maximum value of p is 18.</li>
<li>string : a string of unicode characters.</li>
<li>binary : a string of bytes.</li>
<li>bool : boolean ( true or false ).</li>
</ul>
<p>Each data type has a default value : zero for numbers, a zero length string for string and binary, and false for the boolean type. The variable length data types are stored in special system tables, and are automatically encoded so that only one copy of a given string or binary value is stored.
<h3>ALTER TABLE</h3>
<p>ALTER TABLE schema.tablename action1, action2 .... <p>The actions are as follows:
<ul>
<li>ADD Colname Coltype : a new column is added to the table.</li>
<li>RENAME Colname TO NewColname : the column is renamed.</li>
<li>MODIFY Colname Coltype : the datatype of an existing column is changed. The only changes allowed are between the different sizes of integers, between float and double, and decimals with the same scale.</li>
<li>DROP Colname : the column is removed from the table.</li>
</ul>
<h2>Data manipulation statements</h2>
<h3>INSERT</h3>
<p>INSERT INTO schema.tablename ( Colname1, Colname2 ... ) VALUES ( Val1, Val2... ) [,] ( Val3, Val4 ...) ...
<p>The specified values are inserted into the table. The values may be any expressions ( possibly involving local variables or function calls ).
<p>INSERT INTO schema.tablename ( Colname1, Colname2 ... ) select-expression
<p>The values specified by the select-expression are inserted into the table.
<h3>SELECT</h3><p>SELECT expressions FROM source-table [WHERE bool-exp ] [GROUP BY expressions] [ORDER BY expressions]
<p>A new table is computed, based on the list of expressions and the WHERE, GROUP BY and ORDER BY clauses.
<p>If the keyword DESC is placed after an ORDER BY expression, the order is reversed ( descending order ).
<p>The SELECT expressions can be given names using AS.
<p>source-table can be a named base table, a view or another SELECT enclosed in brackets.
<p>When used as a stand-alone statement, the results are passed to the code that invoked the batch, and may be displayed to a user or sent to a client for further processing and eventual display. 
<p>See the web schema for stored procedures that can be used to generate http responses.
<h3>UPDATE</h3><p>UPDATE schema.tablename SET Colname1 = Exp1, Colname2 = Exp2 .... WHERE bool-exp
<p>Rows in the table which satisfy the WHERE condition are updated.
<h3>DELETE</h3><p>DELETE FROM schema.tablename WHERE bool-exp
<p>Rows in the table which satisfy the WHERE condition are removed.
<h2>Local variable declaration and assignment statements</h2>
<h3>DECLARE</h3><p>DECLARE name1 type1, name2 type2 ....
<p>Local variables are declared with the specified types. Note that the precision makes no difference, tinyint, smallint, int and bigint are all equivalent in this context. The variables are initialised to default values ( but only once, not each time the DECLARE is encountered if there is a loop ).
<h3>SET</h3>
<p>SET name1 = exp1, name2 = exp2 .... [ FROM table ] [ WHERE bool-exp ] [ GROUP BY expressions ]
<p>Local variables are assigned. If the FROM clause is specified, the values are taken from a table row which satisfies the WHERE condition. If there is no such row, the values of the local variables remain unchanged.
<h3>FOR</h3><p>FOR name1 = exp1, name2 = exp2 .... FROM table [ WHERE bool-exp ] [ GROUP BY expressions ] [ORDER BY expressions] Statement
<p>Statement is repeatedly executed for each row from the table which satisfies the WHERE condition, with the named local variables being assigned expressions which depend on the rows.
<h2>Control flow statements</h2>
<h3>BEGIN .. END</h3><p>BEGIN Statement1 Statement2 ... END
<p>The statements are executed in order. A BEGIN..END compound statement can be used whenever a single statement is allowed.
<h3>IF .. THEN ... ELSE ...</h3>
<p>IF bool-exp THEN Statement1 [ ELSE Statement2 ]
<p>If bool-exp evaluates to true Statement1 is executed, otherwise Statement2 ( if specified ) is executed.
<h3>WHILE</h3><p>WHILE bool-exp Statement
<p>Statement is repeatedly executed as long as bool-exp evaluates to true. See also BREAK.
<h3>GOTO</h3><p>GOTO label
<p>Control is transferred to the labelled statement. A label consists of a name followed by a colon (:)
<h3>BREAK</h3><p>BREAK
<p>Execution of the enclosing FOR or WHILE loop is terminated.
<h2>Batch execution</h2><p>EXECUTE ( string-expression )
<p>Evaluates the string expression, and then executes the result ( which should be a list of SQL statements ).
<p>Note that database objects ( tables, views, stored routines ) must be created in a prior batch before being used. A GO statement may be used to signify the start of a new batch.
<h2>Stored Functions</h2>
<h3>CREATE FUNCTION</h3><p>CREATE FUNCTION schema.name ( param1 type1, param2 type2... ) AS BEGIN statements END
<p>A stored function ( no return value ) is created, which can later be called by an EXEC statement.
<h3>EXEC</h3><p>EXEC schema.name( exp1, exp2 ... )
<p>The stored function is called with the supplied parameters.
<h3>Exceptions</h3><p>An exception will terminate the execution of a procedure or EXECUTE batch. EXCEPTION() can be used to obtain a string describing the most recent exception (and clears the exception string). If any exception occurs, the database is left unchanged.
<h3>THROW</h3>
<p>THROW string-expression 
<p>An exception is raised, with the error message being set to the string.
<h3>CREATE FUNCTION</h3><p>CREATE FUNCTION schema.name ( param1 type1, param2 type2... ) RETURNS type AS BEGIN statements END
<p>A stored function is created which can later be used in expressions.
<h3>RETURN</h3>
<p>RETURN expression
<p>Returns a value from a stored function. RETURN with no expression returns from a stored function with no return value.
<p>The pre-defined local variable result can be assigned instead to set the return value.
<h2>Expressions</h2>
<p>Expressions are composed from literals, named local variables, local parameters and named columns from tables or views. These may be combined using operators, stored functions, pre-defined functions. There is also the CASE expression, which has syntax CASE WHEN bool1 THEN exp1 WHEN bool2 THEN exp2 .... ELSE exp END - the result is the expression associated with the first bool expression which evaluates to true.
<h3>Literals</h3>
<p>String literals are written enclosed in single quotes. If a single quote is needed in a string literal, it is written as two single quotes. Binary literals are written in hexadecimal preceded by 0x. Integers are a list of digits (0-9), decimals have a decimal point. The bool literals are true and false.
<h3>Names</h3><p>Names are enclosed in square brackets and are case sensitive ( although language keywords such as CREATE SELECT are case insensitive, and are written without the square brackets, often in upper case only by convention ). The square brackets can be omitted if the name consists of only letters (A-Z,a-z).
<h3>Operators</h3>
<p>The operators ( all binary, except for - which can be unary, and NOT which is only unary ) in order of precedence, high to low, are as follows:
<ul>
<li>*  / % : multiplication, division and remainder (after division) of numbers. Remainder only applies to integers.</li>
<li>+ - : addition, subtraction of numbers.</li>
<li>| : concatenation of strings. The second expression is automatically converted to string if necessary.</li>
<li>= != > < >= <= : comparison of any data type.</li>
<li>IN : tests whether an expression in is in a set. The set may be a list of expressions or a select expression enclosed in brackets.</li>
<li>NOT : boolean negation ( result is true if arg is false, false if arg is true ).</li>
<li>AND : boolean operator ( result is true if both args are true )</li>
<li>OR : boolean operator  ( result is true if either arg is true )</li>
</ul>
<p>Brackets can be used where necessary, for example ( a + b ) * c.
<h3>Pre-defined functions</h3>
<ul>
<li>MIN,MAX,SUM,COUNT : these are used in conjunction with GROUP BY to calculate an aggregate value. If the value of an expression in the SELECT list varies over the grouping, but no aggregate function is specified, the result will be computed from the first input row, prior to grouping - this is probably not useful, but is not an error.</li>
<li>LEN( s string ) : returns the length of s, which must be a string expression.</li>
<li>SUBSTRING( s string, start int, len int ) : returns the substring of s from start (1-based) length len.</li>
<li>REPLACE( s string, pat string, sub string ) : returns a copy of s where every occurrence of pat is replaced with sub.</li>
<li>LASTID() : returns the last Id value allocated by an INSERT statement.</li>
<li>PARSEINT( s string ) : parses an integer from s.</li>
<li>PARSEFLOAT( s string ) : parses a floating point number from s.</li>
<li>PARSEDECIMAL( s string, scale int ) : parses a decimal number from s with the specified scale. The result should be assigned to a decimal variable or table column of matching scale.</li>
<li>EXCEPTION() returns a string with any error that occurred during an EXECUTE statement.</li>
<li>See the web schema for functions that can be used to access http requests.</li>
</ul>
<h3>Conversions</h3>
<p>Any type will implicitly convert to string where required. Integers will convert to float and decimal numbers, and float and decimal will convert to each other as required. ToDo: what about conversions to integer? Truncation vs Rounding etc.
<h2>Views</h2>
<h3>CREATE VIEW</h3>
<p>CREATE VIEW schema.viewname AS SELECT expressions FROM table [WHERE bool-exp ] [GROUP BY expressions]<p>Creates a new view. Every expression must have a unique name.
<h2>Indexes
<h3>CREATE INDEX</h3><p>CREATE INDEX indexname ON schema.tablename( Colname1, Colname2 ... )<p>Creates a new index. Indexes allow efficient access to rows other than by Id values. 
<p>For example, <br>CREATE INDEX ByCust ON dbo.Order(Cust) 
<br>creates an index allowing the orders associated with a particular customer to be efficiently retrieved without scanning the entire order table.
<h2>Rename and Drop</h2>
<h3>RENAME</h3><p>RENAME object-type object-name TO object-name
<p>object-type can be any one of SCHEMA,TABLE,VIEW,PROCEDURE or FUNCTION. The name of the specified object is changed.
<h3>DROP object-type object-name</h3><p>object-type can be any one of SCHEMA,TABLE,VIEW,PROCEDURE or FUNCTION.
<p>The specified object is removed from the database. In the case of a SCHEMA, all objects in the SCHEMA are also removed. In the case of TABLE, all the rows in the table are also removed.
<h3>DROP INDEX</h3><p>DROP INDEX indexname ON schema.tablename<p>The specified index is removed from the database.
<h2>Comments</h2>
<p>There are two kinds of comments. Single line comments start with -- and extend to the end of the line. Delimited comments start with /* and are terminated by */. Comments have no effect, they are simply to help document the code.
<h2>Comparison with other SQL implementations</h2><p>There is a single variable length string datatype \"string\" for unicode strings ( equivalent to nvarchar(max) in MSSQL ), no fixed length strings.
<p>Similarly there is a single binary datatype \"binary\" equivalent to varbinary(max) in MSSQL.
<p>Every table automatically gets an integer Id field ( it does not have to be specified ), which is automatically filled in if not specified in an INSERT statement. Id values must be unique ( an attempt to insert or assign a duplicate Id will raise an exception ). 
<p>WHERE condition is not optional in UPDATE and DELETE statements - WHERE true can be used if you really want to UPDATE or DELETE all rows. This is a \"safety\" feature.
<p>PROCEDURE parameters are in brackets, the procedure body must be enclosed by BEGIN ... END.
<p>Local variables cannot be assigned with SELECT, instead SET or FOR is used, can be FROM a table, e.g.
<p>DECLARE s string SET s = Name FROM sys.Schema WHERE Id = schema
<p>No cursors ( use FOR instead ).
<p>Local variables cannot be assigned in a DECLARE statement.
<p>No default schemas. Schema of tables, routines, functions, views etc. must always be stated explicitly.
<p>No nulls. Columns are initialised to default a value if not specified by INSERT, or when new columns are added to a table by ALTER TABLE.
<p>No triggers. No joins. No outer references.
<h2>Guide to the pre-defined schemas</h2>
<h3>sys</h3>
<p>Has core system tables for language objects and related functions.
<h3>web</h3>
<p>Has the procedure that handles web requests ( web.main ) and other functions related to handling web requests.
<h3>handler</h3>
<p>Has handler procedures, one for each web page.
<h3>htm</h3>
<p>Has functions related to encoding html.
<h3>browse</h3><p>Has tables and functions for displaying, editing arbitrary tables in the database.
<h3>date</h3><p>Has functions for manipulating dates - conversions between Days ( from year 0 ), Year-Day, Year-Month-Day and string.
' 
EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/ListFile]() AS
BEGIN
  EXEC web.Head( 'Files' )
  SELECT '<h1>Files</h1>' 
  SELECT '<p>Path=<a target=_blank href=\"' | Path | '\">' | Path | '</a> Type= ' | ContentType 
   | ' Length=' | ContentLength | ' id=' | Id | ' <a href=\"/EditFile?k=' | Id | '\">Edit Path</a>'
  FROM web.File
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/FileUpload]() AS
BEGIN
  EXEC web.Head( 'File upload' )
  IF FILEATTR(0,0) = 'file' 
  BEGIN
    SELECT '<p>Filename=' | FILEATTR(0,2) | ' ContentType=' | FILEATTR(0,1)
    DECLARE content binary SET content =  FILECONTENT(0)
    
    INSERT INTO web.File( Path, ContentType, ContentLength, Content )
    VALUES ( '/Uploads/' | FILEATTR(0,2), FILEATTR(0,1), LEN(content), content )
  END
  SELECT '<form method=post enctype=\"multipart/form-data\"><p><Input name=file type=file><p><input type=submit value=Upload></form>'
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/Execute]() AS 
BEGIN
  DECLARE sql string SET sql = web.Form('sql')
  EXEC web.Head( 'Execute' )
  SELECT 
     '<p><form method=post>'
     | 'SQL to <input type=submit value=Execute>'
     | '<br><textarea name=sql rows=20 cols=100' | CASE WHEN sql='' THEN ' placeholder=\"Enter SQL here. See Manual for details.\"' ELSE '' END | '>' | htm.Encode(sql) | '</textarea>' 
     | '</form>' 
  IF sql != '' 
  BEGIN
    -- EXEC SETMODE( 1 ) -- Causes result tables to be displayed as HTML tables
    EXECUTE( sql ) 
    -- EXEC SETMODE( 0 )
    DECLARE ex string SET ex = EXCEPTION()
    IF ex != '' SELECT '<p>Error : ' | htm.Encode(ex)
  END
  SELECT '<p>Example SQL:'
     | '<br>SELECT dbo.CustName(Id) AS Name, Age FROM dbo.Cust'
     | '<br>SELECT Cust, Total FROM dbo.Order'
     | '<br>EXEC date.Test( 2020, 1, 1, 60 )'
     | '<br>CREATE TABLE dbo.Cust( LastName string, Age int )'
     | '<br>CREATE INDEX ByLastName on dbo.Cust(LastName)'
     | '<br>CREATE VIEW dbo.OrderSummary AS SELECT Cust, SUM(Total) as Total, COUNT() as Count FROM dbo.Order GROUP BY Cust'
     | '<br>CREATE FUNCTION handler.[/MyPage]() AS BEGIN END'
   EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/EditView]() AS
BEGIN
  DECLARE s string SET s = web.Query('s')
  DECLARE n string SET n = web.Query('n')
  DECLARE sid int SET sid = Id FROM sys.Schema WHERE Name = s
  DECLARE def string, ex string
  SET def = web.Form('def')
  IF def != '' 
  BEGIN
    EXECUTE( 'ALTER VIEW ' | sys.Dot(s,n) | ' AS ' | def )
    SET ex = EXCEPTION()
  END
  ELSE SET def = Def FROM sys.Table WHERE Schema = sid AND Name = n AND IsView = 1
  EXEC web.Head( 'Edit ' | n )
  IF ex != '' SELECT '<p>Error :' | htm.Encode( ex )
  SELECT 
     '<form method=post>'
     | '<input type=submit value=\"ALTER VIEW\"> <a href=ShowSchema?s=' | s | '>' | s | '</a> .' | n | ' AS '
     | '<br><textarea name=def rows=20 cols=100>' | htm.Encode(def) | '</textarea>'
     | '</form>'
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/EditRow]() AS 
BEGIN 
  DECLARE t int SET t = PARSEINT( web.Query('t') )
  DECLARE k int SET k = PARSEINT( web.Query('k') )
  DECLARE ex string
  IF web.Form( '$submit' ) != '' 
  BEGIN
    EXECUTE( browse.UpdateSql( t, k ) ) 
    SET ex = EXCEPTION()
    IF ex = '' 
    BEGIN
      EXEC web.Redirect( 'ShowRow?t=' | t | '&k=' | k )
      RETURN
    END
  END
 
  EXEC web.Head( 'Edit ' | browse.TableTitle( t ) )
  IF ex != '' SELECT '<p>Error: ' | htm.Encode(ex)
  SELECT '<form method=post>' 
  
  EXECUTE( browse.FormUpdateSql( t, k ) )
  SELECT '<p><input name=\"$submit\" type=submit value=Save></form>'
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/EditFunc]() AS
BEGIN
  DECLARE s string SET s = web.Query('s')
  DECLARE n string SET n = web.Query('n')
  DECLARE sid int SET sid = Id FROM sys.Schema WHERE Name = s
  DECLARE def string, ex string SET def = web.Form('def')
  IF def != '' 
  BEGIN
    EXECUTE( 'ALTER FUNCTION ' | sys.Dot(s,n) | def )
    SET ex = EXCEPTION()
  END
  ELSE SET def = Def FROM sys.Function WHERE Schema = sid AND Name = n 
  EXEC web.Head( 'Edit ' | n )
  IF ex != '' SELECT '<p>Error: ' | htm.Encode( ex )
  SELECT 
     '<p><form method=post>'
     | '<input type=submit value=\"ALTER\"> <a href=ShowSchema?s=' | s | '>' | s | '</a> . ' | n 
     | CASE WHEN s = 'handler' THEN ' <a href=' | n | '>Go</a>' ELSE '' END
     | '<br><textarea name=def rows=40 cols=150>' | htm.Encode(def) | '</textarea>' 
     | '</form>' 
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/EditFile]() AS
BEGIN
  DECLARE k int SET k = PARSEINT( web.Query('k') )
  DECLARE path string SET path = web.Form('path')
  IF path != '' UPDATE web.File SET Path = path WHERE Id = k
  EXEC web.Head( 'Edit File' )
  SELECT '<h1>Edit File Path</h1>'
  SELECT '<form method=post>Path: <input name=path size=50 value=\"' | Path | '\">'
    | '<p><input type=submit value=Save></form>'
  FROM web.File WHERE Id = k
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/Dump]() AS 
BEGIN 
  EXEC web.SetContentType( 'text/plain;charset=utf-8' )
  DECLARE s int
  FOR s = Id FROM sys.Schema
    EXEC sys.ScriptSchema(s)
  FOR s = Id FROM sys.Schema
    EXEC sys.ScriptSchemaBrowse(s)
END
GO
CREATE FUNCTION [handler].[/BrowseInfo]() AS 
BEGIN 
  DECLARE k int SET k = PARSEINT( web.Query( 'k' ) )
  DECLARE tid int SET tid = 9
  DECLARE ok int SET ok = 0
  SET ok = Id FROM browse.Table WHERE Id = k
  IF ok != k INSERT INTO browse.Table( Id ) VALUES ( k )
  IF web.Form( '$submit' ) != '' 
  BEGIN
    EXECUTE( browse.UpdateSql( tid, k ) ) 
    EXEC web.Redirect( 'ShowTable?k=' | k )
  END
  ELSE
  BEGIN
    EXEC web.Head( 'Browse Info for ' | sys.TableName(k) )
    SELECT '<form method=post>' 
    EXECUTE( browse.FormUpdateSql( tid, k ) )
    SELECT '<p><input name=\"$submit\" type=submit value=Save></form>'
    EXEC web.Trailer()
  END
END
GO
CREATE FUNCTION [handler].[/BrowseColInfo]() AS 
BEGIN 
  DECLARE tid int SET tid = 8
  DECLARE c int SET c = PARSEINT( web.Query( 'k' ) )
  DECLARE t int, colName string
  SET t = Table, colName = Name FROM sys.Column WHERE Id = c
  DECLARE ok int SET ok = 0
  SET ok = Id FROM browse.Column WHERE Id = c
  IF ok != c INSERT INTO browse.Column( Id ) VALUES ( c )
  IF web.Form( '$submit' ) != '' 
  BEGIN
    EXECUTE( browse.UpdateSql( tid, c ) ) 
    EXEC web.Redirect( 'ShowTable?k=' | t )
  END
  ELSE
  BEGIN
    EXEC web.Head( 'Column ' | colName )
    SELECT '<h1>Column ' | colName | '</h1><form method=post>' 
    EXECUTE( browse.FormUpdateSql( tid, c ) )
    SELECT '<p><input name=\"$submit\" type=submit value=Save></form>'
    EXEC web.Trailer()
  END
END
GO
CREATE FUNCTION [handler].[/AddRow]() AS 
BEGIN 
  DECLARE t int SET t = PARSEINT( web.Query('t') )
  DECLARE ex string
  IF web.Form( '$submit' ) != '' 
  BEGIN
    DECLARE lastid int
    SET lastid = LASTID()
    EXECUTE( browse.InsertSql( t, 0, 0 ) ) 
    SET ex = EXCEPTION()
    IF ex = '' 
    BEGIN
      EXEC web.Redirect( 'ShowRow?t=' | t | '&k=' | LASTID() )
      RETURN
    END
  END
  
  EXEC web.Head( 'Add ' | browse.TableTitle( t ) )
  IF ex != '' SELECT '<p>Error: ' | htm.Encode( ex )
  SELECT '<form method=post>' 
  EXECUTE( browse.FormInsertSql( t, 0 ) )
  SELECT '<p><input name=\"$submit\" type=submit value=Save></form>'
  EXEC web.Trailer()
END
GO
CREATE FUNCTION [handler].[/AddChild]() AS
BEGIN
  DECLARE c int SET c = PARSEINT( web.Query('c') )
  DECLARE p int SET p = PARSEINT( web.Query('p') )
  DECLARE t int SET t = Table FROM sys.Column WHERE Id = c
  DECLARE ex string
  IF web.Form( '$submit' ) != '' 
  BEGIN
    EXECUTE( browse.InsertSql( t, c, p ) ) 
    SET ex = EXCEPTION()
    IF ex = '' 
    BEGIN
      EXEC web.Redirect( 'ShowRow?t=' | t | '&k=' | LASTID() )
      RETURN 
    END
  END
 
  DECLARE title string SET title = 'Add ' | browse.TableTitle( t )
  EXEC web.Head( title )
  SELECT '<b>' | title | '</b><br>'
  IF ex != '' SELECT '<p>Error: ' | ex
  SELECT '<form method=post>' 
  EXECUTE( browse.FormInsertSql( t, c ) )
  SELECT '<p><input name=\"$submit\" type=submit value=Save></form>'
  EXEC web.Trailer()
    
  EXEC web.Trailer()
END
GO
--############################################
CREATE SCHEMA [dbo]
CREATE TABLE [dbo].[Cust]([FirstName] string,[LastName] string,[Age] int,[Postcode] string) 
GO
CREATE INDEX [ByLastName] ON [dbo].[Cust]([LastName])
GO
CREATE TABLE [dbo].[Order]([Cust] int,[Total] decimal(9,2),[Date] int) 
GO
CREATE INDEX [ByCust] ON [dbo].[Order]([Cust])
GO
CREATE FUNCTION [dbo].[test]() AS BEGIN END
GO
CREATE FUNCTION [dbo].[MakeOrders]() AS
BEGIN 
  DELETE FROM dbo.Order WHERE 1 = 1
  DECLARE @I int 
  SET @I=0 
  WHILE @I < 50 -- Use 5000000 to stress system a bit!
  BEGIN 
    INSERT INTO dbo.[Order](Cust,Total) VALUES(1+@I%7, ( 501.00 * (@I%11+@I%7) ) / 100 ) 
    SET @I=@I+1 
  END
END
GO
CREATE FUNCTION [dbo].[CustSelect]( colId int, sel int ) RETURNS string AS
BEGIN
  DECLARE col string SET col = Name FROM sys.Column WHERE Id = colId
  DECLARE opt string, options string
  FOR opt = '<option ' | CASE WHEN Id = sel THEN ' selected' ELSE '' END 
  | ' value=' | Id | '>' | htm.Encode( dbo.CustName(Id) ) | '</option>'
  FROM dbo.Cust
  ORDER BY LastName, FirstName
  SET options |= opt
  RETURN '<select id=\"' | col | '\" name=\"' | col | '\">' | options 
    | '<option ' | CASE WHEN sel = 0 THEN ' selected' ELSE '' END | ' value=0></option>'
    | '</select>'
END
GO
CREATE FUNCTION [dbo].[CustName]( cust int ) RETURNS string AS
BEGIN
  SET result = 'Cust ' | cust -- default in case Cust row does not exist
  SET result = FirstName | ' ' | LastName FROM dbo.Cust WHERE Id = cust
END
GO
INSERT INTO [dbo].[Cust](Id,[FirstName],[LastName],[Age],[Postcode]) VALUES 
(1,'Mary','Poppins',65,'EC4 2NX')
(2,'Clare','Smith',29,'GL3')
(3,'Ron','Jones',45,'')
(4,'Peter','Perfect',36,'')
(5,'George','Washington',25,'WC1')
(6,'Ron','Williams',49,'')
(7,'Adam','Baker',0,'')
(8,'George','Barwood',62,'GL2 4LZ')
GO

INSERT INTO [dbo].[Order](Id,[Cust],[Total],[Date]) VALUES 
(51,1,75,1034482)
(52,2,10,1034273)
(53,3,20,1034273)
(54,4,30,1034273)
(55,1,40,1034273)
(56,1,50,1034451)
(57,1,60,1034338)
(58,1,35,1034273)
(59,2,45,1034273)
(60,3,55,1034273)
(61,4,65,1034273)
(62,1,22,1034437)
(63,1,30,1033842)
(64,7,40,1036563)
(65,1,15,1034273)
(66,2,25,1034273)
(67,3,35,1034273)
(68,4,45,1034273)
(69,5,7,1034273)
(70,6,65,1035809)
(71,7,75,1036097)
(72,1,50,1034273)
(73,2,5,1034273)
(74,3,15,1034273)
(75,4,25,1034273)
(76,5,35,1034273)
(77,1,45,1034785)
(78,7,55,1034273)
(79,1,30,1034273)
(80,2,40,1034273)
(81,3,50,1034273)
(82,1,60,1034465)
(83,2,70,1034273)
(84,6,25,1035297)
(85,7,35,1035297)
(86,1,10,1034273)
(87,2,20,1034273)
(88,3,30,1034273)
(89,4,40,1034273)
(90,1,50,1034273)
(91,6,160,1034465)
(92,1,70,1037195)
(93,1,55,1034285)
(94,2,55,1034273)
(95,3,10,1034273)
(96,4,20,1034273)
(97,5,30,1034273)
(98,6,40,1034785)
(99,7,50,1034785)
(100,1,25,1034273)
(101,1,99,1034273)
(102,5,99,1034273)
(103,4,111,1034273)
(104,1,50,1034273)
(105,1,99,1034273)
(106,1,0,1034273)
(107,1,56,1034273)
(108,5,99,1034273)
(109,5,67,1034273)
(110,5,29,1034273)
(111,1,99,1034273)
(112,4,19,1034273)
(113,4,123,1034273)
(114,1,56,1034273)
(115,1,77,1034273)
(116,1,99,1034461)
(117,1,99,1034465)
GO

--############################################
CREATE SCHEMA [ft]
CREATE FUNCTION [ft].[PersonName]( id int ) RETURNS string AS
BEGIN
  SET result = Firstname | ' ' | Surname | ' ' 
   | CASE WHEN BirthYear > 0 THEN '' | BirthYear ELSE '' END 
   | '-' 
   | CASE WHEN DeathYear > 0 THEN '' | DeathYear ELSE '' END
  FROM ft.Person WHERE Id = id
END
GO
CREATE FUNCTION [ft].[MotherSelect]( colId int, sel int ) RETURNS string AS
BEGIN
  DECLARE col string SET col = Name FROM sys.Column WHERE Id = colId
  DECLARE opt string, options string
  DECLARE by int, k int, ks string SET ks = web.Query( 'k' )
  IF ks != '' SET k = Id, by = BirthYear FROM ft.Person WHERE Id = PARSEINT(ks)  
  
  FOR opt = '<option ' | CASE WHEN Id = sel THEN ' selected' ELSE '' END 
    | ' value=' | Id | '>' | htm.Encode( ft.PersonName(Id) ) | '</option>'
  FROM ft.Person
  WHERE ( NOT Male ) AND Id != k AND ( BirthYear < by - 10 OR by = 0 )
  ORDER BY Surname, Firstname, BirthYear,  BirthMonth, BirthDay
  SET options |= opt
  RETURN '<select id=\"' | col | '\" name=\"' | col | '\">' 
    | options 
    | '<option ' | CASE WHEN sel = 0 THEN ' selected' ELSE '' END | ' value=0></option>'
    | '</select>'
END
GO
CREATE FUNCTION [ft].[FatherSelect]( colId int, sel int ) RETURNS string AS
BEGIN
  DECLARE col string SET col = Name FROM sys.Column WHERE Id = colId
  DECLARE opt string, options string
  DECLARE by int, k int, ks string SET ks = web.Query( 'k' )
  IF ks != '' SET k = Id, by = BirthYear FROM ft.Person WHERE Id = PARSEINT(ks)  
  
  FOR opt = '<option ' | CASE WHEN Id = sel THEN ' selected' ELSE '' END 
    | ' value=' | Id | '>' | htm.Encode( ft.PersonName(Id) ) | '</option>'
  FROM ft.Person
  WHERE Male AND Id != k AND ( BirthYear < by - 10 OR by = 0 )
  ORDER BY Surname, Firstname, BirthYear,  BirthMonth, BirthDay
  SET options |= opt
  RETURN '<select id=\"' | col | '\" name=\"' | col | '\">' 
    | options 
    | '<option ' | CASE WHEN sel = 0 THEN ' selected' ELSE '' END | ' value=0></option>'
    | '</select>'
END
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Column'
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Table'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',2,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Function'
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Schema'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',1,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Index'
INSERT INTO browse.Table(Id,NameFunction, SelectFunction, DefaultOrder, Title, Description, Role) 
VALUES (tid,'sys.IndexName','','','','',0)
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Table'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',2,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'IndexColumn'
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Index'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',4,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Schema'
INSERT INTO browse.Table(Id,NameFunction, SelectFunction, DefaultOrder, Title, Description, Role) 
VALUES (tid,'sys.SchemaName','browse.SchemaSelect','','','',0)
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'sys'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Table'
INSERT INTO browse.Table(Id,NameFunction, SelectFunction, DefaultOrder, Title, Description, Role) 
VALUES (tid,'sys.TableName','browse.TableSelect','','','',0)
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Schema'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',1,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'web'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'File'
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'browse'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Column'
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'RefersTo'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',2,'',0,'',0,0,'','')
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'browse'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Table'
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'dbo'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Cust'
INSERT INTO browse.Table(Id,NameFunction, SelectFunction, DefaultOrder, Title, Description, Role) 
VALUES (tid,'dbo.CustName','dbo.CustSelect','','Customer','',0)
GO
DECLARE tid int, sid int, cid int
SET sid = Id FROM sys.Schema WHERE Name = 'dbo'
SET tid = Id FROM sys.Table WHERE Schema = sid AND Name = 'Order'
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Cust'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',10,'',0,'',0,0,'','')
SET cid=Id FROM sys.Column WHERE Table = tid AND Name = 'Date'
INSERT INTO browse.Column(Id,[Position],[Label],[Description],[RefersTo],[Default],[InputCols],[InputFunction],[InputRows],[Style],[DisplayFunction],[ParseFunction]) 
VALUES (cid, 0,'','',0,'date.DaysToYearMonthDay(date.Today())',0,'browse.InputYearMonthDay',0,0,'date.YearMonthDayToString','date.StringToYearMonthDay')
GO";
