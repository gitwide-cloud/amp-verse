"""Isolated generated-fixture adapter for JSON, v1 and sharded v2 catalogs.
All writers must be stopped. This helper is never used on personal libraries.
"""
import json,sqlite3,tempfile,uuid
from pathlib import Path

def _snapshot(root,name):
 if not name.startswith("songs-") or not name.endswith(".ampdb") or Path(name).name!=name or "/" in name or "\\" in name:
  raise ValueError("Invalid fixture shard")
 path=root/"ampdj/catalogs"/name
 if path.is_symlink():raise ValueError("Fixture shard must not be a symlink")
 return path

def raw(root,key):
 root=Path(root);database=root/"ampdj/catalog.ampdb"
 if database.exists():
  with sqlite3.connect(database.as_uri()+"?mode=ro",uri=True) as db:
   version=db.execute("PRAGMA user_version").fetchone()[0]
   if version==2 and key.startswith("tracks/"):
    row=db.execute("SELECT shards.file FROM locations JOIN shards ON locations.shard=shards.id WHERE locations.key=?",(key,)).fetchone()
    if row:
     with sqlite3.connect(_snapshot(root,row[0]).as_uri()+"?mode=ro",uri=True) as songs:
      value=songs.execute("SELECT document FROM records WHERE key=?",(key,)).fetchone()
      if value:return value[0].encode()
   else:
    value=db.execute("SELECT document FROM records WHERE key=?",(key,)).fetchone()
    if value:return value[0].encode()
 for folder in ("ampdj",".ampverse"):
  path=root/folder/key
  if path.exists():return path.read_bytes()
 raise FileNotFoundError(key)

def read(root,key):return json.loads(raw(root,key))

def write(root,key,value):
 root=Path(root).resolve()
 if not root.is_relative_to(Path(tempfile.gettempdir()).resolve()):raise ValueError("Only isolated temporary test fixtures may be edited")
 if ".." in Path(key).parts or Path(key).is_absolute():raise ValueError("Invalid fixture document")
 data=json.dumps(value);database=root/"ampdj/catalog.ampdb"
 if database.exists():
  with sqlite3.connect(database) as db:
   version=db.execute("PRAGMA user_version").fetchone()[0]
   if version==2 and key.startswith("tracks/"):
    row=db.execute("SELECT shards.id,shards.file FROM locations JOIN shards ON locations.shard=shards.id WHERE locations.key=?",(key,)).fetchone()
    if row is None:row=db.execute("SELECT id,file FROM shards WHERE count<500 ORDER BY id DESC LIMIT 1").fetchone()
    if row is None:
     ident=db.execute("SELECT COALESCE(MAX(id),0)+1 FROM shards").fetchone()[0]
     row=(ident,"songs-%06d-%s.ampdb"%(ident,uuid.uuid4()))
    ident,name=row;path=_snapshot(root,name);path.parent.mkdir(parents=True,exist_ok=True)
    with sqlite3.connect(path) as songs:
     songs.execute("CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY,document TEXT NOT NULL)")
     songs.execute("INSERT INTO records VALUES(?,?) ON CONFLICT(key) DO UPDATE SET document=excluded.document",(key,data))
     count=songs.execute("SELECT COUNT(*) FROM records").fetchone()[0]
     if count>500:raise ValueError("Fixture shard capacity exceeded")
    db.execute("INSERT INTO locations VALUES(?,?) ON CONFLICT(key) DO UPDATE SET shard=excluded.shard",(key,ident))
    db.execute("INSERT INTO shards VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET file=excluded.file,count=excluded.count",(ident,name,count))
   else:db.execute("INSERT INTO records(key,document) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET document=excluded.document",(key,data))
 else:
  path=root/"ampdj"/key;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(data)
