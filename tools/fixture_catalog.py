"""Read/write generated test metadata in either portable storage format.
Writes are restricted to temporary fixture directories and require all AMP workers stopped.
"""
import json,sqlite3,tempfile
from pathlib import Path

def raw(root,key):
 root=Path(root);database=root/"ampdj/catalog.ampdb"
 if database.exists():
  with sqlite3.connect(database.as_uri()+"?mode=ro",uri=True) as db:
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
  with sqlite3.connect(database) as db:db.execute("INSERT INTO records(key,document) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET document=excluded.document",(key,data))
 else:
  path=root/"ampdj"/key;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(data)
