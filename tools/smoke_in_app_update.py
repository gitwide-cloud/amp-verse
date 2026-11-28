# Native transaction acceptance fixture. No user application/state is touched.
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, time

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def compile_fixture(root, label):
    source=root/(label+'.rs');binary=root/(label+('.exe' if os.name=='nt' else ''))
    source.write_text('fn main(){let args:Vec<String>=std::env::args().collect();if args.len()==3{std::fs::write(&args[1],format!("'+label+'\\n{}\\n{}\\n{}",args[2],std::env::var("AMP_UPDATE_FIXTURE_IDENTITY").unwrap_or_default(),std::process::id())).unwrap();}}')
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
            lines=marker.read_text().splitlines()
            assert lines[:3]==[('old' if broken else 'new'),music,'existing-identity-fixture']
            pid=int(lines[3])
            if os.name=='nt':
                import ctypes
                kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.OpenProcess.restype=ctypes.c_void_p
                handle=kernel.OpenProcess(0x00100000,False,pid)
                if handle:
                    try:assert kernel.WaitForSingleObject(ctypes.c_void_p(handle),5000)==0
                    finally:kernel.CloseHandle(ctypes.c_void_p(handle))
                else:assert ctypes.get_last_error()==87
            else:
                end=time.monotonic()+5
                process=pathlib.Path('/proc')/str(pid)/'stat'
                while process.exists() and time.monotonic()<end:
                    try:
                        if process.read_text().split()[2]=='Z':break
                    except (FileNotFoundError,ProcessLookupError):break
                    time.sleep(.01)
            assert digest(app/executable)==digest(old if broken else new)
            assert (app/'settings.json').read_text()=='preserved-library-and-pairing'
            result=(updates/'last-result.txt').read_text();assert ('failed' if broken else 'installed') in result
            assert not stage.exists() and not (jobroot/'download.zip').exists(),'Large download/stage files were retained'
            if server:assert (app/'PENDING_UPDATE.txt').exists(),'Server update modified unrelated desktop state'
            print(('server' if server else 'desktop'),('rollback' if broken else 'upgrade'),'replaced, restarted and preserved identity/settings; download cleanup passed',flush=True)

def headless_server(server):
    import socket, urllib.request, urllib.error
    with tempfile.TemporaryDirectory(prefix='amp-headless-update-') as name:
        root=pathlib.Path(name);music=root/'music';music.mkdir()
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1',0));port=reservation.getsockname()[1]
        env=os.environ.copy();env.update(AMP_CAST_BIND=f'127.0.0.1:{port}',AMP_CAST_CONFIG_DIR=str(root/'config'),AMP_DEVICE_CONFIG_DIR=str(root/'device'),AMP_ANALYSIS_ENABLED='0',AMP_CAST_DISCOVERY_PORT='0',AMP_CAST_PASSWORD='fixture-control')
        with (root/'server.log').open('wb') as log:
            process=subprocess.Popen([str(server.resolve()),str(music)],env=env,stdout=log,stderr=log,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
            try:
                end=time.monotonic()+20;status=None
                while time.monotonic()<end:
                    assert process.poll() is None,'Headless server exited before becoming ready'
                    try:
                        request=urllib.request.Request(f'http://127.0.0.1:{port}/api/update',headers={'x-amp-password':'fixture-control'})
                        with urllib.request.urlopen(request,timeout=1) as response:status=json.load(response)
                        break
                    except (OSError,ValueError):time.sleep(.1)
                assert status and status['available'] and not status['busy']
                time.sleep(.25);assert process.poll() is None,'Missing console incorrectly stopped server'
                request=urllib.request.Request(f'http://127.0.0.1:{port}/api/update/install',data=b'{}',headers={'x-amp-password':'fixture-control','Origin':'https://untrusted.invalid'},method='POST')
                try:urllib.request.urlopen(request,timeout=2);raise AssertionError('Cross-site update was accepted')
                except urllib.error.HTTPError as error:assert error.code==401
                print('real headless server stays online and rejects cross-site update installation',flush=True)
            finally:
                if process.poll() is None:process.terminate()
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:process.kill();process.wait();raise

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--helper',type=pathlib.Path);parser.add_argument('--server-helper',type=pathlib.Path);parser.add_argument('--server',type=pathlib.Path);args=parser.parse_args()
    if not args.helper and not args.server_helper:parser.error('At least one native helper is required')
    if args.helper:exercise(args.helper,False)
    if args.server_helper:exercise(args.server_helper,True)

    if args.server:headless_server(args.server)
