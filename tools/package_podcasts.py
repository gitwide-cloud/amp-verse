"""Publish a versioned, checksummed optional source-provider package."""
import argparse,json,pathlib,hashlib,re
p=argparse.ArgumentParser();p.add_argument('--version',required=True);p.add_argument('--destination',required=True);a=p.parse_args()
if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+_-]{0,63}',a.version) or '..' in a.version:raise SystemExit('Invalid plugin version')
out=pathlib.Path(a.destination);out.mkdir(parents=True,exist_ok=True)
name=f'amp-podcasts-{a.version}.ampplugin'
data=(json.dumps(dict(id='amp-podcasts',version=a.version,host_api=1))+'\n').encode()
(out/name).write_bytes(data);(out/(name+'.sha256')).write_text(hashlib.sha256(data).hexdigest()+'  '+name+'\n')
