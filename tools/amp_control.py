"""Send one show action with a retry-safe UUID. Uses only Python's standard library."""
import argparse,json,os,sys,time,urllib.request,urllib.error,uuid
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("command",help='JSON action, e.g. {"action":"pause","deck":"auto"}')
parser.add_argument("--url",default="http://127.0.0.1:8790")
parser.add_argument("--password",default=os.environ.get("AMP_CONTROL_PASSWORD",""))
parser.add_argument("--id",help="Reuse this ID only when retrying the same physical command")
args=parser.parse_args()
body=json.loads(args.command);body["id"]=args.id or str(uuid.uuid4())
request=urllib.request.Request(args.url.rstrip("/")+"/api/v1/command",data=json.dumps(body).encode(),headers={"Content-Type":"application/json","x-amp-password":args.password})
for attempt in range(3):
    try:
        with urllib.request.urlopen(request,timeout=8) as reply: print(reply.read().decode());sys.exit(0)
    except urllib.error.HTTPError as error: print(error.read().decode(),file=sys.stderr);sys.exit(1)
    except OSError:
        if attempt==2:raise
        time.sleep(.25)
