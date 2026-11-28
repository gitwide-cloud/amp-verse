# Native transaction acceptance fixture. No user application/state is touched.
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, time

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def compile_fixture(root, label):
    source=root/(label+'.rs');binary=root/(label+('.exe' if os.name=='nt' else ''))
    source.write_text('fn main(){let args:Vec<String>=std::env::args().collect();if args.len()==3{std::fs::write(&args[1],format!("'+label+'\\n{}\\n{}",args[2],std::env::var("AMP_UPDATE_FIXTURE_IDENTITY").unwrap_or_default())).unwrap();}}')
    subprocess.run(['rustc',str(source),'-o',str(binary)],check=True,timeout=60,capture_output=True)
    return binary

def exercise(helper,server):
    helper=helper.resolve();assert helper.is_file()
    with tempfile.TemporaryDirectory(prefix='amp-update-acceptance-') as name:
        root=pathlib.Path(name);old=compile_fixture(root,'old');new=compile_fixture(root,'new')
        for broken in [False,True]:
            case=root/('rollback' if broken else 'upgrade');case.mkdir();app=case/'application';app.mkdir();updates=case/'updates';updates.mkdir();jobroot=pathlib.Path(tempfile.mkdtemp(prefix='.tmp',dir=updates));stage=jobroot/'files';stage.mkdir()
            executable=('amp-cast' if server else 'amp-player')+('.exe' if os.name=='nt' else '')
            shutil.copy2(old,app/executable);shutil.copy2(new,stage/executable)
            (app/'settings.json').write_text('preserved-library-and-pairing');(app/'PENDING_UPDATE.txt').write_text('old desktop update')
            (jobroot/'download.zip').write_bytes(b'fixture-download')
            marker=case/'restart.txt';music='Existing music folder with spaces';files=[executable]+(['missing-file'] if broken else [])
            plan={'parent':0,'destination':str(app),'stage':str(stage),'files':files,'result':str(updates/'last-result.txt'),'server':server,'arguments':[str(marker),music]}
            job=jobroot/'job.json';job.write_text(json.dumps(plan));runner=jobroot/helper.name;shutil.copy2(helper,runner)
            env=os.environ.copy();env['AMP_UPDATE_FIXTURE_IDENTITY']='existing-identity-fixture';env['TEMP']=str(case);env['TMPDIR']=str(case)
            run=subprocess.run([str(runner),str(job)],env=env,timeout=30,capture_output=True)
            assert run.returncode==(1 if broken else 0),(run.returncode,run.stdout,run.stderr)
            end=time.monotonic()+10
            while not marker.exists() and time.monotonic()<end:time.sleep(.05)
            assert marker.read_text()==('old' if broken else 'new')+'\n'+music+'\nexisting-identity-fixture'
            assert digest(app/executable)==digest(old if broken else new)
            assert (app/'settings.json').read_text()=='preserved-library-and-pairing'
            result=(updates/'last-result.txt').read_text();assert ('failed' if broken else 'installed') in result
            assert not stage.exists() and not (jobroot/'download.zip').exists(),'Large download/stage files were retained'
            if server:assert (app/'PENDING_UPDATE.txt').exists(),'Server update modified unrelated desktop state'
            print(('server' if server else 'desktop'),('rollback' if broken else 'upgrade'),'replaced, restarted and preserved identity/settings; download cleanup passed',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--helper',type=pathlib.Path);parser.add_argument('--server-helper',type=pathlib.Path);args=parser.parse_args()
    if not args.helper and not args.server_helper:parser.error('At least one native helper is required')
    if args.helper:exercise(args.helper,False)
    if args.server_helper:exercise(args.server_helper,True)
