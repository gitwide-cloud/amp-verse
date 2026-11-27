"""Publish the frozen, fully uploaded RC6 only after checking its exact asset inventory."""
import json,os,urllib.request
from pathlib import Path
REPO='gitwide-cloud/amp-verse'; RELEASE=461106859
SOURCE='2f4ce593523e6719c9dae61c152c64181dfec74c';TREE='fcb40437c1beeb8840187db1c45de75ba1c8feca'
def verify_assets(expected,actual):
 if len({x['name'] for x in expected})!=len(expected) or len({x['name'] for x in actual})!=len(actual):raise ValueError('Duplicate asset names')
 if {x['name'] for x in expected}!={x['name'] for x in actual}:raise ValueError('Missing or unexpected release assets')
 by_name={x['name']:x for x in actual}
 for row in expected:
  a=by_name[row['name']]
  if a['state']!='uploaded' or a['size']!=row['size'] or a.get('digest')!='sha256:'+row['sha256']:raise ValueError('Release asset integrity mismatch: '+row['name'])
def main():
 if os.environ.get('GITHUB_REPOSITORY')!=REPO or os.environ.get('GITHUB_EVENT_NAME')!='push' or os.environ.get('GITHUB_REF')!='refs/heads/fix/publish-rc6':raise ValueError('Unexpected publication context')
 token=os.environ['GH_TOKEN']
 def api(path,data=None):
  req=urllib.request.Request('https://api.github.com/repos/'+REPO+'/'+path,data=None if data is None else json.dumps(data).encode(),method='GET' if data is None else 'PATCH',headers={'Authorization':'Bearer '+token,'Accept':'application/vnd.github+json','User-Agent':'AMP-verified-release-publisher','Content-Type':'application/json'})
  with urllib.request.urlopen(req,timeout=90) as f:return json.load(f)
 for run_id in (46068873744,46068873752,46069021505,46068873708,46068873674,46068873671):
  run=api('actions/runs/'+str(run_id))
  if run['status']!='completed' or run['conclusion']!='success':raise ValueError('Required platform qualification failed')
 for commit in (SOURCE,'c4364b6a4d69f09306cae5c91b9ef416ef91f7d9','263042ffeff55102b17dd3875caec4c741b6bf51'):
  if api('git/commits/'+commit)['tree']['sha']!=TREE:raise ValueError('Binary and tagged product sources differ')
 expected=json.loads((Path(__file__).resolve().parents[1]/'deploy/releases/rc6-inventory.json').read_text())
 r=api('releases/'+str(RELEASE))
 if r['tag_name']!='v1.1.0-rc.6' or r['target_commitish']!=SOURCE or not r['prerelease']:raise ValueError('Unexpected release identity/channel')
 verify_assets(expected,api('releases/'+str(RELEASE)+'/assets?per_page=100'))
 if r['draft']:r=api('releases/'+str(RELEASE),{'draft':False,'prerelease':True,'make_latest':'false'})
 if r['draft'] or not r['published_at']:raise ValueError('Release was not published')
 print(json.dumps({'url':r['html_url'],'published':True,'assets_verified':len(expected),'version':'1.1.0-rc.6'}))
if __name__=='__main__':main()
